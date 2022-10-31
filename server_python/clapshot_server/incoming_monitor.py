"""Incoming folder monitor."""

import logging
from multiprocessing import Queue
from pathlib import Path
import time
from typing import DefaultDict

from .multi_processor import install_sigterm_handlers


def monitor_incoming_folder_loop(
    incoming_dir: Path,
    files_to_process: Queue,
    poll_interval: float,
    resubmit_delay: float) -> None:
    """
    Monitor a folder and put new files to processing queue.
    Waits for a file to be fully written (not growing) before putting it to the queue.

    Args:
        incoming_dir:       Path to the incoming/ folder
        files_to_process:   Queue to put new files to
        poll_interval:      How often to check for new files (in seconds)
        resubmit_delay:     How long to wait before resubmitting a file if it was already submitted.
                            This basically specifies how long processing of a file should take at maximum.
    """

    install_sigterm_handlers()

    try:
        logger = logging.getLogger("incoming")

        incoming = Path(incoming_dir)
        assert incoming.is_dir(), f"Path '{incoming}' is not a directory."
        logger.info(f"Starting incoming folder monitor in '{incoming}'...")

        last_tested_size: DefaultDict[Path, int] = DefaultDict(int) # For detecting files that are still being written to
        submission_time: DefaultDict[Path, float] = DefaultDict(float)

        while True:
            logger.debug("Checking for new files...")

            # Remove expired submissions
            submission_time = {k: v for k, v in submission_time.items() if time.time() - v < resubmit_delay}

            # Check for new files in the incoming folder
            for fn in incoming.iterdir():
                if fn.is_file() and not submission_time.get(fn):

                    # Check if file is still being written to
                    cur_size = fn.stat().st_size
                    if cur_size > 1 and cur_size != 4096:  # 4096 is the size of an empty file on ext4
                        if cur_size == last_tested_size[fn]:
                            logger.info(f"Submitting '{fn}' for processing. ")
                            files_to_process.put(str(fn.absolute()))
                            submission_time[fn] = time.time()
                            del last_tested_size[fn]
                        else:
                            logger.info(f"File '{fn}' size changed since last poll. Skipping it for now...")
                            last_tested_size[fn] = cur_size

            # Wait for a bit before checking again
            time.sleep(poll_interval)

    except KeyboardInterrupt:
        pass

    logger.info("Incoming monitor stopped.")
