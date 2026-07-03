"""Channel allowlist matching and path safety."""

import logging
import re
from collections.abc import Sequence
from pathlib import Path

log = logging.getLogger(__name__)


def is_channel_allowed(channel_id: str, channel_name: str, allowed: Sequence[str]) -> bool:
    for rule in allowed:
        rule = rule.strip()
        if rule.startswith("name:"):
            if re.search(rule[5:], channel_name):
                return True
        elif channel_id == rule:
            return True
    return False


def safe_path(base: Path, *parts: str) -> Path | None:
    """Join path parts and verify the result stays within base directory."""
    result = (base / Path(*parts)).resolve()
    if not result.is_relative_to(base.resolve()):
        log.warning("Path traversal blocked: %s", result)
        return None
    return result
