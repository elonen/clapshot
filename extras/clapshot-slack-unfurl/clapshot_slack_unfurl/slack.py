"""Slack API abstraction layer."""

import json
import logging
import time
from typing import Any

from slack_sdk import WebClient
from slack_sdk.errors import SlackApiError
from .cache import TTLCache

log = logging.getLogger(__name__)

# Cache for Slack file IDs: (file_path, mtime) → slack file ID
_file_cache: TTLCache[tuple[str, float], str] = TTLCache(maxsize=256)

# Cache for channel names: channel_id → channel_name
_channel_cache: TTLCache[str, str] = TTLCache(maxsize=256, ttl=900)


def resolve_channel_name(client: WebClient, channel_id: str) -> str:
    """Get channel name from ID, with caching. Returns "" if unresolvable."""
    cached = _channel_cache.get(channel_id)
    if cached is not None:
        return cached
    try:
        resp = client.conversations_info(channel=channel_id)
        name: str = resp["channel"]["name"]
        _channel_cache.put(channel_id, name)
        return name
    except Exception:
        log.info("Could not resolve channel name for %s (bot may not be a member); "
                 "only channel ID rules will be checked", channel_id)
        _channel_cache.put(channel_id, "")
        return ""


def upload_image(client: WebClient, image_bytes: bytes, filename: str,
                 file_path_key: str, file_mtime: float) -> str | None:
    """Upload image to Slack, return file ID. Uses cache to avoid re-uploads."""
    cache_key = (file_path_key, file_mtime)
    cached = _file_cache.get(cache_key)
    if cached is not None:
        return cached
    try:
        resp = client.files_upload_v2(content=image_bytes, filename=filename)
        file_id: str = resp["file"]["id"]
        _file_cache.put(cache_key, file_id)
        return file_id
    except Exception:
        log.exception("Failed to upload image to Slack")
        return None


def send_unfurl(client: WebClient, channel: str, ts: str, url: str,
                title: str, fields: list[tuple[str, str]],
                text: str | None, file_id: str | None) -> None:
    """Send chat.unfurl with Block Kit content.

    Uses an image block for the thumbnail/drawing, then a section with
    fields for compact two-column metadata display.

    If an image is included and the unfurl is rejected (file may not be fully
    processed yet), retries with exponential backoff before falling back to
    a text-only unfurl.
    """
    blocks: list[dict[str, Any]] = []
    if file_id:
        blocks.append({
            "type": "image",
            "slack_file": {"id": file_id},
            "alt_text": title,
        })

    # Title + optional freeform text (e.g. comment body)
    header = f"*{title}*"
    if text:
        header += f"\n{text}"
    blocks.append({
        "type": "section",
        "text": {"type": "mrkdwn", "text": header},
    })

    # Metadata fields in two-column layout
    if fields:
        blocks.append({
            "type": "section",
            "fields": [
                {"type": "mrkdwn", "text": f"*{label}:* {value}"}
                for label, value in fields
            ],
        })

    unfurls: dict[str, dict[str, Any]] = {url: {"blocks": blocks}}
    log.debug("chat.unfurl payload: %s", json.dumps(unfurls, indent=2))

    # Retry with backoff when an image is present — Slack may not have finished
    # processing the uploaded file yet. Falls back to text-only after all retries.
    delays = [1, 2, 4, 8, 16] if file_id else []
    last_error = ""
    for attempt, delay in enumerate([0] + delays):
        if delay:
            log.debug("chat.unfurl attempt %d failed (%s), retrying in %ds",
                      attempt, last_error, delay)
            time.sleep(delay)
        try:
            resp = client.chat_unfurl(channel=channel, ts=ts, unfurls=unfurls)
            if resp.get("ok"):
                return
            last_error = str(resp.get("error", "unknown"))
        except SlackApiError as e:
            last_error = str(e.response.get("error", e))
        except Exception:
            log.exception("chat.unfurl failed")
            return

    if file_id:
        log.warning("chat.unfurl with image failed after %d attempts (%s), falling back to text-only",
                    len(delays) + 1, last_error)
        fallback_blocks = [b for b in blocks if b["type"] != "image"]
        try:
            client.chat_unfurl(channel=channel, ts=ts, unfurls={url: {"blocks": fallback_blocks}})
        except Exception:
            log.exception("chat.unfurl text-only fallback also failed")
    else:
        log.warning("chat.unfurl failed: %s", last_error)


def send_ephemeral(client: WebClient, channel: str, user: str, text: str) -> None:
    """Temporary message to user, e.g. for errors."""
    try:
        client.chat_postEphemeral(channel=channel, user=user, text=text)
    except Exception:
        log.exception("chat_postEphemeral failed")
