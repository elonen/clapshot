"""
Message queue manager that passes messages between API, metadata reader, ingestion and transcoding.
"""
import logging
from pathlib import Path
import multiprocessing
from multiprocessing import Process
import time

from . import video_metadata_reader
from . import video_compressor
from .incoming_monitor import monitor_incoming_folder_loop
from .multi_processor import install_sigterm_handlers
from .video_ingesting import VideoIngestingPool, IngestingArgs, UserResults


class VideoProcessingPipeline():
    """
    Message queue manager.
    """
    def __init__(self, db_file: str, data_dir: Path, mpm: multiprocessing.Manager, max_workers: int = 0):
        """
        Create a dispatcher that manages the message queues between the various worker pools.

        Args:
            db_file:      Path to the SQLite database file
            data_dir:     Path to the data directory (incoming, videos, reject)
            mpm:          Python multiprocessing manager
            max_workers:  Max number of workers to use per component. 0 = use all available CPUs.
                          The manager itself is single-threaded, this is only for subservient worker pools.
        """
        self.mpm = mpm
        self.max_workers = max_workers

        self.files_from_incoming = mpm.Queue()
        self.files_to_process = mpm.Queue()
        self.res_to_user = mpm.Queue()
        self.to_compress = mpm.Queue()
        self.compress_results = mpm.Queue()
        self.to_metadata_reader = mpm.Queue()
        self.metadata_results = mpm.Queue()

        self.db_file = db_file
        self.videos_dir = data_dir / "videos"
        self.incoming_dir = data_dir / "incoming"
        self.reject_dir = data_dir / "rejected"


    def queue_for_ingestion(self, videofile: Path, user_id: str):
        """
        Queue a video file for ingestion.
        This gets called when API server receives an upload.

        Args:
            videofile:  Path to the video file
            user_id:    User ID
        """
        self.to_metadata_reader.put(video_metadata_reader.Args(src=videofile, user_id=user_id))


    def run_forever(self, poll_interval: float = 5.0, resubmit_delay: float = 15.0) -> None:
        """
        Run the message queue manager forever.

        Args:
            poll_interval:   How often to poll the message queues for new messages
            resubmit_delay:  How long to wait before resubmitting a file to the ingestion pool
                             if it's still in the incoming folder.
        """

        install_sigterm_handlers()
        logger = logging.getLogger("pipeline")

        try:
            # Start monitoring incoming/
            Process(target=monitor_incoming_folder_loop, args=[], kwargs={
                "incoming_dir": self.incoming_dir,
                "files_to_process": self.files_from_incoming,
                "poll_interval": poll_interval,
                "resubmit_delay": resubmit_delay}, daemon=True).start()

            # Start metadata readers
            Process(target=video_metadata_reader.ReaderPool(
                inq = self.to_metadata_reader,
                outq = self.metadata_results,
                max_workers=self.max_workers).run_forever).start()

            # Start video compressors
            Process(target=video_compressor.CompressorPool(
                inq = self.to_compress,
                outq = self.compress_results,
                max_workers=self.max_workers).run_forever).start()

            # Start video ingestion
            Process(target=VideoIngestingPool(
                inq = self.files_to_process,
                outq = self.res_to_user,
                compress_q = self.to_compress,
                db_file = self.db_file,
                videos_dir = self.videos_dir,
                reject_dir = self.reject_dir,
                max_workers=self.max_workers).run_forever).start()
 
            # Helper: Log results and send to user
            def send_to_user(res: UserResults):
                if res.success:
                    logger.info(f"Processed #{res.video_hash} ({res.orig_file.name}) ok. " + str(res.msg))
                else:
                    logger.error(f"Failed to proc #{res.video_hash} ({res.orig_file.name}): " + str(res.msg))
                self.res_to_user.put(res)

            # Listen to queues
            time_to_sleep = 0
            def _select():
                # Currently there's no proper select() for mp.queues, so
                # emulate with sleep()ing busyloop
                nonlocal time_to_sleep
                res = (q for q in [
                    self.files_from_incoming,
                    self.metadata_results,
                    self.compress_results] if not q.empty())
                if not res:
                    time_to_sleep = min(0.2, (time_to_sleep + 0.001)*1.1)
                else:
                    time_to_sleep = 0.01
                time.sleep(time_to_sleep)
                return res

            while True:
                for q in _select():
                    if not (o := q.get()):
                        continue

                    # Incoming file -> Read metadata
                    if q is self.files_from_incoming:
                        self.to_metadata_reader.put(video_metadata_reader.Args(Path(o)))

                    # Metadata received -> Process and store
                    elif q is self.metadata_results:
                        self.files_to_process.put(IngestingArgs(md=o))

                    # Compression results -> pass back to ingestion
                    elif q is self.compress_results:
                        if not o.success:
                            send_to_user(UserResults(
                                success = False, orig_file = o.orig_file,
                                msg = o.msg, details = o.details,
                                file_owner_id = o.user_id))
                        else:
                            self.files_to_process.put(IngestingArgs(cmpr=o))

                    else:
                        raise Exception("Unknown queue in select()")

        except KeyboardInterrupt:
            logger.info("Video processing pipeline (dispatcher) stopped.")
