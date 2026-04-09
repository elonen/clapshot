"""Logging setup with SIGHUP-based log reopen."""

import logging
import signal
import sys
import types

_file_handler: logging.FileHandler | None = None


def setup(log_path: str | None = None, debug: bool = False) -> None:
    level = logging.DEBUG if debug else logging.INFO
    fmt = "%(asctime)s %(levelname)s %(name)s: %(message)s"

    root = logging.getLogger()
    root.setLevel(level)
    root.handlers.clear()

    if log_path:
        global _file_handler
        _file_handler = logging.FileHandler(log_path)
        _file_handler.setFormatter(logging.Formatter(fmt))
        root.addHandler(_file_handler)
        signal.signal(signal.SIGHUP, _reopen)
    else:
        h = logging.StreamHandler(sys.stdout)
        h.setFormatter(logging.Formatter(fmt))
        root.addHandler(h)


def _reopen(signum: int, frame: types.FrameType | None) -> None:
    if _file_handler:
        _file_handler.close()
        _file_handler.stream = open(_file_handler.baseFilename, "a")
