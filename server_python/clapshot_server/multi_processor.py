"""
Boilderplate code to run functions in a pool of separate processes.
Used for metadata extraction, file ingestion, video compression.
"""
from abc import abstractmethod, ABC
import logging
import multiprocessing
import os
import signal
import sys
import time
from typing import Any

exiting = False
def _term_handler(signal_received, frame):
    global exiting
    if not exiting:
        exiting = True
        raise KeyboardInterrupt()

def install_sigterm_handlers():
    """Facilitate shutdown by CTR-C and SIGTERM"""
    signal.signal(signal.SIGTERM, _term_handler)
    signal.signal(signal.SIGINT, _term_handler)
    signal.signal(signal.SIG_IGN, _term_handler)


class MultiProcessor(ABC):
    """
    Base class for multiprocessing videos with queues.
    """

    def __init__(self, 
        inq: multiprocessing.Queue,
        logging_name: str,
        max_workers: int = 0):
            self.logging_name = logging_name
            self.logger = logging.getLogger(logging_name)
            self.inq = inq
            self.max_workers = max_workers or multiprocessing.cpu_count()


    @abstractmethod
    def do_task(self, args: Any, logging_name: str):   # pragma: no cover
        """
        Abstract method to be implemented by subclasses.

        Args:
            args: Arguments to be passed to the task. (Type depends on the subclass.)
            logging_name: Name of the logger to be used by the task.
        """
        pass


    def _worker(self, p: int):
        worker_name = f"{self.logging_name}.{p+1}"
        logging.getLogger(worker_name).debug(f"Worker {p+1} started, pid {os.getpid()}")
        try:
            install_sigterm_handlers()
            while True:
                if o := self.inq.get():
                    self.do_task(o, worker_name)
                else:
                    time.sleep(0.01)
        except (ConnectionResetError, EOFError, BrokenPipeError, KeyboardInterrupt):
            pass
        finally:
            logging.getLogger(worker_name).debug(f"Worker {p+1} stopped.")
            sys.exit(0)
            

    def run_forever(self):
        """
        Infinitely running loop for multiprocessing.
        Starts a process pool and waits for tasks to be added to the queue.
        """
        n_procs = min(self.max_workers, max(2, int(os.cpu_count()/2)))
        self.logger.info(f"Starting {n_procs} workers...")
        procs = []
        try:
            install_sigterm_handlers()
            procs = [multiprocessing.Process(
                        target=self._worker,
                        name=f'{self.logging_name}_{i}',
                        args=[i])
                    for i in range(n_procs)]

            for p in procs:
                p.start()
            for p in procs:
                p.join()
        except (ConnectionResetError, EOFError, BrokenPipeError, KeyboardInterrupt):
            pass
        finally:
            self.logger.info(f"Main '{self.logging_name}' process stopped.")
            for p in procs:
                p.terminate()
