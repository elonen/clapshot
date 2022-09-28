#!/usr/bin/env python3

from abc import abstractmethod, ABC
import logging
import multiprocessing
import os
import signal
import sys
from typing import Any

exiting = False
def term_handler(signal_received, frame):
    global exiting
    if not exiting:
        exiting = True
        raise KeyboardInterrupt()

def install_sigterm_handlers():
    signal.signal(signal.SIGTERM, term_handler)
    signal.signal(signal.SIGINT, term_handler)
    signal.signal(signal.SIG_IGN, term_handler)


class MultiProcessor(ABC):
    """
    Base class for multiprocessing videos with in/out queues.
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
    def do_task(self, args: Any, logging_name: str):
        pass # pragma: no cover


    def worker(self, p: int):
        worker_name = f"{self.logging_name}.{p+1}"
        logging.getLogger(worker_name).debug(f"Worker {p+1} started, pid {os.getpid()}")
        try:
            install_sigterm_handlers()
            while True:
                if o := self.inq.get():
                    self.do_task(o, worker_name)
        except (ConnectionResetError, EOFError, BrokenPipeError, KeyboardInterrupt):
            pass
        finally:
            logging.getLogger(worker_name).debug(f"Worker {p+1} stopped.")
            sys.exit(0)

    def run_forever(self):
        """
        Infinitely running main function for video compression.
        Starts a thread pool and waits for video compressions tasks to be added to the queue.
        """
        n_procs = min(self.max_workers, max(2, int(os.cpu_count()/2)))
        self.logger.info(f"Starting {n_procs} workers...")
        procs = []
        try:
            install_sigterm_handlers()
            procs = [multiprocessing.Process(
                        target=self.worker,
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
