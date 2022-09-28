#!/usr/bin/env python3

from dataclasses import dataclass
from decimal import Decimal
import hashlib
import logging
from pathlib import Path
import shutil
import traceback
from typing import Optional
from uuid import uuid4
import json
import asyncio
from multiprocessing import Queue

from . import video_metadata_reader
from . import video_compressor
from . import database as DB
from .multi_processor import MultiProcessor


TARGET_VIDEO_MAX_BITRATE = 2.5*(10**6)


@dataclass
class UserResults:
    success: bool
    orig_file: str
    video_hash: str = ""
    msg: str = ""
    details: str = ""
    file_owner_id: str = ""


@dataclass
class IngestingArgs:
    md: video_metadata_reader.Results = None
    cmpr: video_compressor.Results = None
    test_mock: dict = None


class VideoIngestingPool(MultiProcessor):

    def __init__(self,
            inq: Queue,
            outq: Queue,
            compress_q: Queue,
            db_file: str,
            videos_dir: str,
            reject_dir: str,
            max_workers: int = 0):

        super().__init__(inq, "ingest", max_workers)
        self.outq = outq
        self.compress_q = compress_q
        self.db_file = db_file
        self.videos_dir = Path(videos_dir)
        self.reject_dir = Path(reject_dir)


    def do_task(self, args: IngestingArgs, logging_name: str):
        assert args.md or args.cmpr
        if args.md:
            self.outq.put(self.on_recv_metadata(args.md, logging_name, args.test_mock))
        elif args.cmpr:
            self.on_recv_compressor_results(args.cmpr, logging_name)


    def on_recv_metadata(self, md: video_metadata_reader.Results, logging_name: str, test_mock = None) -> UserResults:
        """
        Process a single video:
            - Calculate video hash
            - Create directory for video
            - Move original video into directory
            - Write metadata to database
            - Schedule recompressing if needed
        """
        src = Path(md.src_file)
        logger = logging.getLogger(logging_name)
        logger.debug(f"Got metadata for '{str(src)}'...")
        video_hash = ''
        test_mock = test_mock or {}

        try:
            # If metadata read failed, just clean up and return error
            if not md.success:
                raise Exception(f"{md.msg}: {md.details}")

            assert md.user_id
            assert src.exists(), f"File '{str(src)}' does not exist."
            
            video_hash = self._calc_video_hash(src, md.user_id)
            new_dir = self.videos_dir / video_hash

            logger.debug(f"Video_hash for '{src}' = '{video_hash}. New dir: '{new_dir}'")

            if test_mock.get('preexisting_dir'):
                new_dir.mkdir(parents=True, exist_ok=True)

            # Check if video is already processed
            if new_dir.exists():
                prev_owner = self._get_db_video_owner(video_hash, logger)
                if prev_owner:
                    assert prev_owner == md.user_id, \
                        f"Hash collision?!? Video '{video_hash}' already owned by '{prev_owner}'."

                    # User is trying to upload the same video again. This is ok.
                    src.unlink()
                    return UserResults(
                        success=True,
                        msg=f"You already have this video.",
                        orig_file=md.src_file,
                        file_owner_id=md.user_id,
                        video_hash=video_hash)
                else:
                    logger.warning(f"Dir for '{video_hash}' exists, but not in DB. Deleting old and reprocessing.")
                    shutil.rmtree(new_dir)

            src = self._create_videodir_and_move_orig(src, new_dir, logger)
            self._write_metadata_to_db(md, video_hash, logger)


            # Schedule recompression if needed
            new_bitrate = self._calc_recompression_bitrate(md)
            if new_bitrate:
                self.compress_q.put( video_compressor.Args(
                        src = src,
                        dst = new_dir / f'temp_{uuid4()}.mp4',
                        video_bitrate = new_bitrate,
                        video_hash = video_hash,
                        user_id = md.user_id
                    ))

            return UserResults(
                success = True,
                orig_file = md.src_file,
                file_owner_id = md.user_id,
                video_hash = video_hash,
                msg=f"Video added. " + (
                    "Transcoding it..." if new_bitrate 
                    else "Original codec ok."))

        except Exception as e:
            logger.error(f"Error processing '{str(src)}': {e}")

            # Try to clean up
            cleanup_err = None
            try:
                self._cleanup_and_move_to_rejected(
                    orig_src=Path(src),
                    video_hash=video_hash,
                    videos_dir=Path(self.videos_dir),
                    reject_dir=Path(self.reject_dir))
            except Exception as e2:
                logger.error(f"Cleanup error for '{str(src)}': {e2}")
                cleanup_err = e2

            # tb = traceback.format_exception(type(e), e, e.__traceback__)
            tb = ''

            return UserResults(
                orig_file=str(src),
                file_owner_id=md.user_id or '',
                video_hash=video_hash,
                success=False,
                msg=f"Video ingesting failed." + (" Cleanup failed also." if cleanup_err else ''),
                details=str(e) + str(tb) + (("\n--- cleanup ---\n" + str(cleanup_err)) if cleanup_err else ''))

    def on_recv_compressor_results(self, c: video_compressor.Results, logger: logging.Logger) -> None:
        """
        Handle compression results.
        Switches the original file with the compressed one, stores logs and updates DB.
        """
        def _post_results(success: bool, msg: str='', details: str=''):
            self.outq.put(UserResults(
                success = success,
                orig_file = c.src_file,
                video_hash = c.video_hash,
                msg = msg,
                details = details,
                file_owner_id = c.user_id))
        
        if c.success:
            vid_dir = self.videos_dir /  c.video_hash

            # Store FFmpeg logs
            try:
                if vid_dir.exists():
                    with open(vid_dir / "ffmpeg.stdout.txt", "w") as f:
                        f.write(c.stdout)
                    with open(vid_dir / "ffmpeg.stderr.txt", "w") as f:
                        f.write(c.stderr)
            except Exception as e:
                _post_results(False, msg="Failed to store ffmpeg logs.", details=str(e))

            try:
                # Symlink compressed file
                dst_file = vid_dir / "video.mp4"
                if dst_file.exists():
                    dst_file.unlink()
                dst_file.symlink_to(c.dst_file)

                try:
                    # Update DB
                    async def mark_recompressed():
                        async with DB.Database(Path(self.db_file), logger) as db:
                            assert not db.error_state, f"DB error state {db.error_state}"
                            await db.set_video_recompressed(c.video_hash)
                    asyncio.run(mark_recompressed())
                except Exception as e:
                    _post_results(False, msg="Failed to mark video as recompressed.", details=str(e))

            except Exception as e:
                _post_results(False, msg="Symlink to recompressed video failed.", details=str(e))

        _post_results(True, msg=c.msg, details=c.details)


    # -----------------------------
    # Helpers
    # -----------------------------

    def _calc_video_hash(self, fn: Path, user_id: str) -> str:
        """
        Calculate identifier ("video_hash") for the submitted video,
        based on filename, user_id, size and sample of the file contents.
        """
        file_hash = hashlib.sha256((str(fn) + str(user_id) + str(fn.stat().st_size)).encode('utf-8'))
        assert fn.lstat().st_size > 0, f"File '{fn}' is empty."
        with open(fn, 'rb') as f:
            file_hash.update(f.read(32*1024))
        hash = file_hash.hexdigest()
        assert len(hash) >= 8
        return hash[:8]


    def _create_videodir_and_move_orig(self, src: Path, new_dir: Path, logger: logging.Logger) -> Path:
        """
        Create the new directory for the video, and move the original file into it.
        Returns:
            Path to the original video in the new directory
        """
        logger.debug(f"Creating dir '{new_dir}'...")
        new_dir.mkdir(parents=False, exist_ok=True)

        dir_for_orig = new_dir / "orig"
        assert not (dir_for_orig / src.name).exists(), f"File '{src.name}' already exists in '{dir_for_orig}'. Aborting."
        logger.debug(f"Creating dir '{dir_for_orig}'...")
        dir_for_orig.mkdir(parents=False)

        logger.debug(f"Moving '{src}' to '{dir_for_orig}'...")
        shutil.move(src, dir_for_orig)
        assert (dir_for_orig / src.name).exists(), f"Failed to move '{src}' to {dir_for_orig}. Aborting."
        return dir_for_orig / src.name


    def _get_db_video_owner(self, video_hash: str, logger: logging.Logger) -> str:
        """
        Get the user_id of the video as recorded in the database.
        """
        async def lookup_existing():
            async with DB.Database(Path(self.db_file), logger) as db:
                return await db.get_video(video_hash)
        old_vid = asyncio.run(lookup_existing())
        return old_vid.added_by_userid if old_vid else None


    def _write_metadata_to_db(self, md: video_metadata_reader.Results, video_hash: str, logger: logging.Logger):
        """
        Write the video metadata to the database.
        Raises:
            AssertionError: If the database write fails
        """
        try:
            logger.debug(f"Writing metadata to database...")
            async def add_video_to_db():
                logger.debug(f"Opening DB '{self.db_file}'...")
                async with DB.Database(Path(self.db_file), logger) as db:
                    assert not db.error_state, f"DB error state {db.error_state}"
                    logger.debug(f"db.add_video ...")
                    await db.add_video(DB.Video(
                        video_hash = video_hash,
                        added_by_userid = md.user_id,
                        added_by_username = md.user_id,       # TODO: get username from user id (wrap LDAP in some kind of abstraction)
                        orig_filename = md.src_file.name,
                        total_frames = md.total_frames,
                        duration = Decimal(md.duration),
                        fps = str(md.fps),
                        raw_metadata_all = json.dumps(md.metadata_all)
                    ))
            asyncio.run(add_video_to_db())
            logger.debug(f"Metadata wrote to DB.")
        except Exception as e:
            raise Exception(f"DB error writing metadata: {e}")


    def _calc_recompression_bitrate(self, md: video_metadata_reader.Results) -> Optional[int]:
        """
        Determine if the video needs to be recompressed.
        Bitrate and format are checked.
        Return:
            The bitrate to use for the recompression, or None if no recompression is needed.
        """
        new_bitrate = max(int(md.bitrate/2), min(int(md.bitrate), TARGET_VIDEO_MAX_BITRATE))
        already_fine = (new_bitrate >= md.bitrate or md.bitrate <= 1.2*TARGET_VIDEO_MAX_BITRATE) and \
            md.orig_codec.lower() in ('h264', 'hevc', 'h265', 'avc') and \
            Path(md.src_file).suffix.lower() in ('.mp4', '.mkv')
        return None if already_fine else new_bitrate


    def _cleanup_and_move_to_rejected(self,
        orig_src: Path, video_hash: Optional[str],
        videos_dir: Path, reject_dir: Path) -> None:
        """
        Move the video to the rejected directory, and delete the video hash directory (if it exists).

        Args:
            orig_src:       Original source path
            video_hash:     ID/hash of the video
            videos_dir:     Directory for succesfully processed videos
            reject_dir:     Directory for rejected videos
        
        Raises:
            AssertionError: If cleanup fails
        """
        if video_hash:
            hash_dir = videos_dir / video_hash
            if hash_dir.exists():
                file_to_move = hash_dir / "orig" / orig_src.name
                move_to_dir = reject_dir / video_hash
                if file_to_move.exists():
                    move_to_dir.mkdir(parents=False, exist_ok=True)
                    assert not (move_to_dir / orig_src.name).exists(), f"File '{orig_src.name}' already exists in '{move_to_dir}'. Aborting cleanup."
                    shutil.move(file_to_move, move_to_dir)
                    assert not file_to_move.exists(), f"Failed to move '{file_to_move}' - still exists. Aborting cleanup."
                    assert (move_to_dir / orig_src.name).exists(), f"DISASTER! File '{file_to_move}' disappeared after moving it to '{move_to_dir}'! Aborting cleanup."
                if not file_to_move.exists():
                    shutil.rmtree(hash_dir)
                    assert not hash_dir.exists(), f"Failed to delete '{hash_dir}' - still exists. Please delete manually."
        else:
            if orig_src.exists():
                assert reject_dir.exists() and reject_dir.is_dir(), f"Reject directory '{reject_dir}' does not exist. Aborting cleanup."
                assert not (reject_dir / orig_src.name).exists(), f"File '{orig_src.name}' already exists in '{reject_dir}'. Aborting cleanup."
                shutil.move(orig_src, reject_dir)
                assert not orig_src.exists(), f"Failed to move '{orig_src}' into {reject_dir}. Please move manually."
                assert (reject_dir / orig_src.name).exists(), f"DISASTER! File '{orig_src}' disappeared after moving it to '{reject_dir}'! Please investigate."
