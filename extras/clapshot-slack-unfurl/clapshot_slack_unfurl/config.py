"""TOML config with env var override and CLI arg parsing."""

import argparse
import logging
import os
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

log = logging.getLogger(__name__)


@dataclass
class Config:
    videos_dir: Path
    sqlite_path: Path
    slack_bot_token: str
    slack_app_token: str
    allowed_channels: list[str] = field(default_factory=list)
    image_min_width: int | None = None
    image_max_width: int | None = None
    debug: bool = False
    log_path: str | None = None


def _die(msg: str) -> None:
    """Log a fatal config error and exit."""
    log.critical(msg)
    raise SystemExit(1)


def load(argv: list[str] | None = None) -> Config:
    # Set up minimal logging so config errors are visible in journalctl
    logging.basicConfig(format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(description="Clapshot Slack link unfurler")
    parser.add_argument("-c", "--config", default="clapshot-slack-unfurl.toml",
                        help="Config file path (default: clapshot-slack-unfurl.toml)")
    parser.add_argument("--debug", action="store_true", help="Enable debug logging")
    parser.add_argument("--log", metavar="PATH", help="Log to file instead of stdout")
    args = parser.parse_args(argv)

    path = Path(args.config)
    if not path.exists():
        _die(f"Config file not found: {path}")

    try:
        with open(path, "rb") as f:
            raw: dict[str, Any] = tomllib.load(f)
    except tomllib.TOMLDecodeError as e:
        _die(f"Failed to parse {path}: {e}")

    errors: list[str] = []
    videos_dir = raw.get("videos_dir")
    if not videos_dir:
        errors.append("videos_dir is required")
    sqlite_path = raw.get("sqlite_path")
    if not sqlite_path:
        errors.append("sqlite_path is required")
    bot_token = os.environ.get("SLACK_BOT_TOKEN") or raw.get("slack_bot_token", "")
    if not bot_token:
        errors.append("SLACK_BOT_TOKEN env var or slack_bot_token in config is required")
    app_token = os.environ.get("SLACK_APP_TOKEN") or raw.get("slack_app_token", "")
    if not app_token:
        errors.append("SLACK_APP_TOKEN env var or slack_app_token in config is required")
    if errors:
        for err in errors:
            log.critical("Config error: %s", err)
        raise SystemExit(1)

    assert videos_dir is not None  # checked above
    assert sqlite_path is not None
    return Config(
        videos_dir=Path(videos_dir),
        sqlite_path=Path(sqlite_path),
        slack_bot_token=bot_token,
        slack_app_token=app_token,
        allowed_channels=raw.get("allowed_channels", []),
        image_min_width=raw.get("image_min_width"),
        image_max_width=raw.get("image_max_width"),
        debug=args.debug or raw.get("debug", False),
        log_path=args.log or raw.get("log_path"),
    )
