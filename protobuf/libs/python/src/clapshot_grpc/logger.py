import json
import logging
import sys


def make_organizer_logger(name: str, debug: bool = False, json: bool = False) -> logging.Logger:
    """
    Create a Clapshot Server -compatible logger for an Organizer plugin.

    The embeds organizer log entries within its own log best when the log is in a certain format.
    This function creates a standard logger that outputs in that format.

    :param name: Name of the logger
    :param debug: Whether to enable debug logging
    :param json: Whether to log in JSON format
    """
    logger = logging.getLogger(name)
    logger.setLevel(logging.DEBUG if debug else logging.INFO)
    formatter = _JsonFormatter() if json else logging.Formatter(f'%(levelname)s [%(name)s] %(message)s')  # no timestamp, it's already logged by the server

    # Create a stream handler for stdout (for levels below ERROR)
    stdout_handler = logging.StreamHandler(sys.stdout)
    stdout_handler.setLevel(logging.DEBUG if debug else logging.INFO)
    stdout_handler.addFilter(lambda record: record.levelno < logging.ERROR)
    stdout_handler.setFormatter(formatter)
    logger.addHandler(stdout_handler)

    # Create a stream handler for stderr (for levels ERROR and above)
    stderr_handler = logging.StreamHandler(sys.stderr)
    stderr_handler.setLevel(logging.ERROR)
    stderr_handler.setFormatter(formatter)
    logger.addHandler(stderr_handler)

    return logger


class _JsonFormatter(logging.Formatter):
    def format(self, record):
        log_record = {
            'time': self.formatTime(record, self.datefmt),
            'level': record.levelname,
            'name': record.name,
            'message': record.getMessage(),
        }
        return json.dumps(log_record)
