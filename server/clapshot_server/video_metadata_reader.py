"""
Pool of processes that read metadata from video files.
"""

from dataclasses import dataclass
from decimal import Decimal
import logging
from pathlib import Path
import multiprocessing
from pymediainfo import MediaInfo
from .multi_processor import MultiProcessor


@dataclass
class Args:
    """Arguments for the video metadata reader."""
    src: Path
    user_id: str = None
    test_mock: dict = None


@dataclass
class Results:
    """Resulting metadata or error messages."""
    success: bool
    msg: str = ""
    details: str = ""
    src_file: str = ""
    user_id: str = ""
    total_frames: int = -1
    duration: Decimal = -1
    orig_codec: str = ""
    fps: str = ""
    bitrate: int = -1
    metadata_all: dict = None


class ReaderPool(MultiProcessor):
    """
    Metadata reader pool.
    """

    def __init__(self, inq: multiprocessing.Queue, outq: multiprocessing.Queue, max_workers: int = 0):
        """
        Create a new pool of video metadata readers.

        Args:
            inq (Queue[Args]):         Queue of Args objects to process.
            outq (Queue[Results]):     Queue of Results objects to return.
            max_workers (int):         Maximum number of workers to spawn. If 0, use the number of CPUs.
        """
        super().__init__(inq, "metad", max_workers)
        self.outq = outq

    def do_task(self, args: Args, logging_name: str) -> None:
        self.outq.put(self.read_metadata(args, logging_name))


    def read_metadata(self, args: Args, logging_name: str) -> Results:
        """
        Read metadata from a given video file.

        Args:
            args (Args):        Arguments object to process.
            logging_name (str): Name of the logger to use.
        
        Returns:
            Results:            Results object with the results of the operation.
                                Contains read metadata or error details if the operation failed.
        """
        src = args.src
        test_mock = args.test_mock or {}        
        user_id = args.user_id or (src.owner() if src.exists() else '')

        try:
            logger = logging.getLogger(logging_name)
            logger.debug(f"Reading metadata for '{src}'...")

            video = None
            mediainfo = MediaInfo.parse(src.absolute())
            for track in mediainfo.tracks:
                if track.track_type == "Video" and not test_mock.get('no_video_stream'):
                    for x in ('frame_count', 'frame_rate', 'height', 'width', 'duration', 'format'):
                        if x not in track.to_data() or test_mock.get('missing_mediainfo_fields'):
                            raise ValueError(f"No field '{x}' in video track")                    
                    video = track
                    break

            if not video:
                return Results(
                    success=False, src_file=src, user_id=user_id,
                    msg="Metadata error.",
                    details=f"No video stream found in '{src}'.")

        except Exception as e:
                return Results(
                    success=False, src_file=src, user_id=user_id,
                    msg=f"Error reading mediainfo.", details=str(e))

        # Calc duration and bitrate (if not found in mediainfo)
        duration_sec = Decimal(video.duration) / Decimal(1000)
        bit_rate = video.to_data().get('bit_rate') or video.to_data().get('nominal_bit_rate')
        if not bit_rate or test_mock.get('no_bit_rate'):
            logger.warning(f"No bit rate found for '{src}'. Calculating it from file size.")
            bit_rate = int(src.stat().st_size * 8 / duration_sec)

        logger.debug(f"Metadata for '{src}': codec='{video.format}', fps='{video.frame_rate}', bit_rate='{int(bit_rate)}', frame_count='{video.frame_count}', duration='{duration_sec}'")

        return Results(
            success = True,
            src_file = src,
            user_id = user_id,
            metadata_all = mediainfo.to_data(),
            total_frames = int(video.frame_count),
            duration = Decimal(duration_sec),
            orig_codec = video.format,
            bitrate = int(bit_rate),
            fps = str(video.frame_rate))
