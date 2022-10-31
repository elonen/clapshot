"""
Transcoder for making submitted videos smaller and more compatible with browsers.
This runs a pool of separate processes and invokes FFMPEG to do the work.
"""

from dataclasses import dataclass
import logging
from pathlib import Path
import ffmpeg
import multiprocessing

from .multi_processor import MultiProcessor

TARGET_AUDIO_BITRATE = 128*(10**3)
TARGET_VIDEO_MAX_W = '1920'


@dataclass
class Args:
    """Arguments for the video compressor."""
    src: Path
    dst: Path
    video_bitrate: int
    video_hash: str
    user_id: str = None


@dataclass
class Results:
    """Results from the video compressor."""
    src_file: str
    dst_file: str
    video_hash: str
    success: bool
    msg: str = ""
    details: str = ""
    stdout: str = ""
    stderr: str = ""
    user_id: str = ""


class CompressorPool(MultiProcessor):
    """
    A pool of workers that transcode videos with FFMPEG.
    """

    def __init__(self, inq: multiprocessing.Queue, outq: multiprocessing.Queue, max_workers: int = 0):
        """
        Create a new pool of video compressor workers.

        Args:
            inq (Queue[Args]):         Queue of Args objects to process.
            outq (Queue[Results]):     Queue of Results objects to return.
            max_workers (int):         Maximum number of workers to spawn. If 0, use the number of CPUs.
        """
        super().__init__(inq, "compr", max_workers)
        self.outq = outq


    def do_task(self, args: Args, logging_name: str) -> None:
        self.outq.put(self.compress(args, logging_name))

    
    def compress(self, args: Args, logging_name: str) -> Results:
        """
        Recompress video with FFMPEG.
        """
        src = args.src
        dst = args.dst
        bitrate = args.video_bitrate

        try:
            logger = logging.getLogger(logging_name)
            logger.info(f"Converting '{src}' to '{dst}'...")
            assert src.exists(), "Source file does not exist"
            assert not dst.exists(), "Destination file already exists"

            logger.info(f"Transcoding '{src}' with new bitrate {bitrate} as '{dst}'...")
            out, err = ffmpeg \
                .input(filename=src.absolute()) \
                .output(filename=dst.absolute(), 
                    vcodec='libx264', preset='faster', 
                    vf=f'scale={TARGET_VIDEO_MAX_W}:-8',
                    map=0,          # copy all streams
                    acodec='aac',
                    ac=2,           # stereo
                    strict='experimental',
                    **{'b:v': bitrate, 'b:a': TARGET_AUDIO_BITRATE}) \
                .global_args('-nostdin', '-hide_banner', '-nostats') \
                .overwrite_output()  \
                .run(capture_stdout=True, capture_stderr=True)

            logger.debug(f"FFMPEG done")

            return Results(
                success=True,
                msg="Video transcoded.",
                video_hash=args.video_hash, user_id=args.user_id,
                src_file=str(src.absolute()), dst_file=str(dst.absolute()),
                stdout=out.decode('utf-8'),  stderr=err.decode('utf-8'))

        except Exception as e:
            logger.error(f"Error converting video #{args.video_hash} '{src}' to '{dst}': {e}")
            return Results(
                success=False,
                video_hash=args.video_hash, user_id=args.user_id,
                src_file=str(src.absolute()), dst_file=str(dst.absolute()),
                msg="Video compression failed.", details=str(e),
                stdout=bytes(e.stdout).decode('utf-8'), stderr=bytes(e.stderr).decode('utf-8'))
