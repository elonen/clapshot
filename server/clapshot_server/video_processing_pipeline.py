#!/usr/bin/env python3

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

    def __init__(self, db_file: str, data_dir: Path, mpm: multiprocessing.Manager, max_workers: int = 0):
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


    def run_forever(self, poll_interval: float = 5.0, resubmit_delay: float = 15.0) -> None:
        install_sigterm_handlers()
        logger = logging.getLogger("dispatch")

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
            def _select():
                res = (q for q in [
                    self.files_from_incoming,
                    self.metadata_results,
                    self.compress_results] if not q.empty())
                if not res:
                    # Currently there's no proper select() for mp.queues, so
                    # emulate with sleep()ing busyloop
                    time.sleep(0.01)
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
