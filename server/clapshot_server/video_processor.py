#!/usr/bin/env python3

from decimal import Decimal
from fractions import Fraction
import hashlib
import multiprocessing as mp
import logging
import os
from pathlib import Path
import queue
import shutil
import threading
from typing import Any, Callable, DefaultDict, Optional
import ffmpeg
import json
import asyncio

from . import database as DB

TARGET_VIDEO_MAX_BITRATE = 2.5*(10**6)
TARGET_AUDIO_BITRATE = 128*(10**3)
TARGET_VIDEO_MAX_W = '1920'

logging.basicConfig(level=logging.DEBUG)


# Used for returning multiprocessing results in a queue
class ProcessingResult:
    def __init__(self, orig_file: Path, file_owner_id: str, success: bool, msg: str = None, video_hash: str = None):
        self.orig_file = orig_file
        self.video_hash = video_hash
        self.file_owner_id = file_owner_id
        self.success = success
        self.msg = msg
    
    def __repr__(self) -> str:
        return f"ProcessingResult(orig_file={self.orig_file}, video_hash={self.video_hash}, file_owner_id={self.file_owner_id}, success={self.success}, msg={self.msg})"

class VideoProcessor:
    def __init__(self, db_file: Path, logger: logging.Logger = None) -> None:
        self.logger = logger or logging.getLogger("clapshot.videoproc")
        self.db_file = db_file


    def convert_video(self, src: Path, dst: Path, logger: logging.Logger, orig_bit_rate: int, orig_codec: str) \
        -> Optional[tuple[Path, Path]]:
        """
        Convert & scale down video to with ffmpeg-python, if necessary.

        Args:
            src: Path to the source video file
            dst: Path to the destination video file
            logger: logger to use
            orig_bit_rate: original video bit rate (for skipping conversion if not necessary)
            orig_codec: original video codec (for skipping conversion if not necessary)

        Returns:
            tuple(stdout_log: Path, stderr_log: Path) -- logs from FFmpeg, or None if no conversion was necessary

        Raises:
            Exception: if the conversion fails (also writes ffmpeg output to a log)
        """

        logger.info(f"Converting '{src}' to '{dst}'...")
        assert src.exists()

        fn_stdout = dst.parent / 'encoder.stdout'
        fn_stderr = dst.parent / 'encoder.stderr'

        newbitrate = max(int(orig_bit_rate/2), min(int(orig_bit_rate), TARGET_VIDEO_MAX_BITRATE))
        if newbitrate >= orig_bit_rate and orig_codec.lower() in ('h264', 'hevc', 'h265') and src.name.lower().endswith('mp4'):
            logger.info(f"Keeping original video codec '{orig_codec}'/MP4 because new bitrate is lower than original. Copying instead of transcoding.")
            shutil.copy(src, dst)
            logger.info(f"Video copied ok'")
        else:
            try:
                out, err = ffmpeg \
                    .input(filename=src.absolute()) \
                    .output(filename=dst.absolute(), 
                        vcodec='libx264', preset='faster', 
                        vf=f'scale={TARGET_VIDEO_MAX_W}:-8',
                        map=0,          # copy all streams
                        acodec='aac',
                        ac=2,           # stereo
                        strict='experimental',
                        **{'b:v': newbitrate, 'b:a': TARGET_AUDIO_BITRATE}) \
                    .global_args('-nostdin', '-hide_banner', '-nostats') \
                    .overwrite_output()  \
                    .run(capture_stdout=True, capture_stderr=True)

                fn_stdout.write_bytes(out or b'')
                fn_stderr.write_bytes(err or b'')
                logger.info(f"Conversion done")
                
                return fn_stdout, fn_stderr

            except ffmpeg.Error as e:
                fn_stdout.write_bytes(bytes(e.stdout))
                fn_stderr.write_bytes(bytes(e.stderr))
                msg = f"Error converting video '{src}' to '{dst}'. See '{fn_stderr}' and '{fn_stdout}' for details."
                logger.error(msg)
                raise Exception(msg)
        return None


    def read_video_metadata(self,
        src: Path,
        video_hash: str,
        logger: logging.Logger,
        fmt_result: Callable[[str, bool], ProcessingResult],
        test_mock: dict = {}) \
            -> tuple[Optional[ProcessingResult], str, int]:
        """
        Read video metadata with ffmpeg-python and write it to the database.

        Args:
            src: Path to the source video file
            video_hash: hash (unique id) of the video file
            logger: logger to use
            fmt_result: function to post a ProcessingResult, if error occurs
            test_mock: mock values for testing
        
        Returns:
            tuple(result: ProcessingResult, orig_codec: str, orig_bitrate: int) -- result is None if no error occurred
        """

        try:
            metadata = ffmpeg.probe(src.absolute())
        except ffmpeg.Error as e:
            return fmt_result(f"FFMPEG error reading video metadata for '{src}': {e}", False), 'None', 0

        # (TESTING: delete video streams if requested by pytest)
        if test_mock.get('no_video_stream'):
            metadata['streams'] = [s for s in metadata['streams'] if s['codec_type'] != 'video']

        # Get video stream metadata
        video_stream = next((stream for stream in metadata['streams'] if stream['codec_type'] == 'video'), None)
        if not video_stream:
            return fmt_result("No video stream found in the file. Giving up.", False), 'None', 0

        total_frames = int(video_stream['nb_frames'])
        duration = Decimal(video_stream['duration'])
        codec = video_stream['codec_name']
        fps =  Fraction(video_stream.get('avg_frame_rate')) if not ('no_fps' in test_mock) else None
        bit_rate = int(video_stream.get('bit_rate') or '0')
        if not fps:
            fps = Fraction(total_frames) / Fraction(duration)

        try:
            async def add_video_to_db():
                async with DB.Database(Path(self.db_file), logger) as db:
                    await db.add_video(DB.Video(
                        video_hash=video_hash,
                        added_by_userid=src.owner(),
                        added_by_username=src.owner(),       # TODO: get username from user id (wrap LDAP in some kind of abstraction)
                        orig_filename=src.name,
                        total_frames=total_frames,
                        duration=duration,
                        fps=str(fps.numerator / Decimal(fps.denominator)),
                        raw_metadata_video=json.dumps(video_stream),
                        raw_metadata_all=json.dumps(metadata)
                    ))
            asyncio.run(add_video_to_db())
        except Exception as e:
            return fmt_result(f"Error inserting video info into DB: {e}", False), codec, bit_rate

        return None, codec, bit_rate


    def process_file(self, src: Path, dst_dir: Path) -> ProcessingResult:
        """
        Process a video file: recompress and get metadata.            
        Args:
            src: Path to the source video file
            dst_dir: Path to the destination directory

        Returns:
            ProcessingResult
        """
        logger = logging.getLogger(f"clapshot.videoproc.worker_pid{os.getpid()}")
        try:
            # Name the file with a hash of the first 128k of file contents + the original filename
            file_hash = hashlib.md5(str(src).encode('utf-8'))
            with open(src, 'rb') as f:
                file_hash.update(f.read(128*1024))
            assert len(file_hash.hexdigest()) >= 8
            new_dir = dst_dir / file_hash.hexdigest()[:8]

            # Helper for returning results through multiporcessing queue
            def fmt_result(msg: str, success: bool) -> ProcessingResult:
                if success:
                    logger.info(f"Succesfully processed '{src}' -> '{new_dir}'")
                else:
                    logger.error(f"Error processing '{src}' -> '{new_dir}': {msg}")
                return ProcessingResult(
                    orig_file=src,
                    file_owner_id=src.owner(),
                    success=success,
                    video_hash=new_dir.name,
                    msg=msg)

            # Move video to video dir
            dir_for_orig = new_dir / "orig"
            assert not (dir_for_orig / src.name).exists(), f"File '{src}' already exists in '{dir_for_orig}'. Aborting."
            
            logger.info(f"Moving '{src}' to '{dir_for_orig}/'...")
            new_dir.mkdir(parents=False, exist_ok=True)
            dir_for_orig.mkdir(parents=False, exist_ok=True)
            shutil.move(src, dir_for_orig)
            assert (dir_for_orig / src.name).exists(), f"Failed to move '{src}' to {dir_for_orig}. Aborting."
            src = dir_for_orig / src.name       # update src to point to the new location

            opt_res, orig_codec, orig_bitrate = self.read_video_metadata(src, new_dir.name, logger, fmt_result)
            if opt_res:
                assert not opt_res.success, "read_video_metadata should not return success"
                return opt_res
            
            # Convert video to mp4 with ffmpeg
            mp4_file = new_dir / "video.mp4"
            self.convert_video(src, mp4_file, logger, orig_bitrate, orig_codec)

            return fmt_result("Video processing complete", True)

        except Exception as e:
            logger.error(f"Generic video processing error '{str(src)}' to : {e}")
            return ProcessingResult(
                orig_file=src,
                file_owner_id=src.owner(),
                success=False,
                msg=f"Generic video processing error: {e}")


    def cleanup_and_move_to_rejected(self, orig_src: Path, video_hash: Optional[str], dst_dir: Path, reject_dir: Path) -> None:
        """
        Move the video to the rejected directory, and delete the video hash directory (if it exists).

        Args:
            orig_src:       Original source path
            video_hash:     ID/hash of the video
            dst_dir:        Directory for succesfully processed videos
            reject_dir:     Directory for rejected videos
        
        Raises:
            AssertionError: If cleanup fails
        """        
        if video_hash:
            hash_dir = dst_dir / video_hash
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


    def monitor_incoming_folder_loop(self,
        incoming_dir: Path, 
        dst_dir: Path,
        rejected_dir: Path,
        interrupt_flag: threading.Event,
        results: queue.Queue,
        poll_interval: int,
        test_mock: dict = {}) -> None:
        """
        Monitor the incoming folder for new files and process them.
        This is a blocking function that runs in a separate thread.
        It spawns a new process for each file it finds as soon as it determines that the file is not being written to anymore.

        Args:
            incoming_dir:    Incoming videos directory
            dst_dir:         Where to store the processed videos
            rejected_dir:    Where to move rejected videos
            interrupt_flag:  Event to signal process should be interrupted
            results:         Queue to post ProcessingResults to
            poll_interval:   How often to check for new files (in seconds)        
        """
        logger = logging.getLogger(f"clapshot.videoproc.incoming_monitor")

        incoming = Path(incoming_dir)
        logger.info(f"Starting incoming folder ({incoming}) monitor")

        last_tested_size: DefaultDict[Path, int] = DefaultDict(int) # For detecting files that are still being written to
        skip_list: set[Path] = set()    # For skipping files that failed to process before (and could not be moved to rejected)

        if test_mock.get("test_skip_list"):
            skip_list.add(Path("non-existent-file"))

        with mp.Pool() as pool:
            while not interrupt_flag.is_set():
                logger.debug("Checking for new files...")
                process_now = []

                # Clean up skip_list (remove files that no longer exist)
                skip_list = set(filter(lambda x: x.exists(), skip_list))

                # Check for new files in the incoming folder
                for fn in incoming.iterdir():
                    if fn.is_file() and fn not in skip_list:
                        # Check if file is still being written to
                        cur_size = fn.stat().st_size
                        if cur_size == last_tested_size[fn]:
                            logger.info(f"File '{fn}' not growing any more. Processing it...")
                            process_now.append(fn)
                        else:
                            logger.info(f"File '{fn}' size changed since last poll. Skipping it for now...")
                            last_tested_size[fn] = cur_size

                # Process new files in parallel and wait for them to finish
                # (otherwise we might process the same file twice)
                if process_now:
                    for r in pool.starmap(self.process_file, [(src, dst_dir) for src in process_now]):
                        if not r.success:
                            logger.error(f"Failed to process '{r.orig_file}': {r.msg}. Cleaning up...")
                            try:
                                self.cleanup_and_move_to_rejected(r.orig_file, r.video_hash, dst_dir, rejected_dir)
                            except Exception as e:
                                logger.error(f"Failed to cleanup after processing '{r.orig_file}':: {e}")
                                r.msg = f"{r.msg}. ALSO, failed to cleanup: {e}"
                            if r.orig_file.exists():
                                logger.error(f"File '{r.orig_file}' still exists after cleanup. Adding to skip_list, so we don't reprocess it.")
                                skip_list.add(r.orig_file)
                        results.put(r)

                interrupt_flag.wait(timeout=poll_interval)

        logger.info("Video processor stopped")
