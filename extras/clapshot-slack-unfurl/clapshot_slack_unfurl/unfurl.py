"""URL parsing, DB lookup, and unfurl content assembly."""

import logging
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import urlparse, parse_qs

from .acl import safe_path
from .db import CommentRow, open_db, get_media_file, get_comment
from .media import to_jpeg_bytes, humanize_duration

log = logging.getLogger(__name__)


@dataclass
class ParsedURL:
    video_id: str
    comment_id: str | None = None


@dataclass
class UnfurlResult:
    title: str
    fields: list[tuple[str, str]]  # (label, value) pairs for compact display
    text: str | None  # optional freeform text (e.g. comment body)
    image_bytes: bytes | None
    image_filename: str
    image_source_path: Path | None


def parse_clapshot_url(url: str) -> ParsedURL | None:
    """Extract video_id and optional comment_id from a Clapshot URL."""
    parsed = urlparse(url)
    params = parse_qs(parsed.query)
    vid = params.get("vid", [None])[0]
    if not vid:
        return None
    comment_id = None
    if parsed.fragment and parsed.fragment.startswith("comment_"):
        comment_id = parsed.fragment[len("comment_"):]
    return ParsedURL(video_id=vid, comment_id=comment_id)


def build_unfurl(url: str, db_path: Path, videos_dir: Path,
                 image_min_width: int | None = None,
                 image_max_width: int | None = None) -> UnfurlResult | None:
    """Build unfurl data for a Clapshot URL. Returns None if not found."""
    parsed = parse_clapshot_url(url)
    if not parsed:
        return None

    try:
        conn = open_db(db_path)
        video = get_media_file(conn, parsed.video_id)
        if not video:
            return None

        comment: CommentRow | None = None
        if parsed.comment_id:
            comment = get_comment(conn, parsed.comment_id)
        conn.close()
    except Exception:
        log.exception("DB error during unfurl lookup")
        return None

    # Determine image: drawing (if comment has one) or video thumbnail.
    # Then convert to JPEG bytes for Slack upload.
    image_bytes: bytes | None = None
    image_filename: str = f"{parsed.video_id}_thumb.jpg"
    image_source_path: Path | None = None
    try:
        if comment and comment.drawing:
            image_source_path = safe_path(videos_dir, parsed.video_id, "drawings", comment.drawing)
            if image_source_path:
                image_bytes = to_jpeg_bytes(image_source_path, min_width=image_min_width, max_width=image_max_width)
                image_filename = f"{parsed.video_id}_drawing_{comment.id}.jpg"
        if not image_bytes:
            image_source_path = safe_path(videos_dir, parsed.video_id, "thumbs", "thumb.webp")
            if image_source_path:
                image_bytes = to_jpeg_bytes(image_source_path, min_width=image_min_width, max_width=image_max_width)
    except Exception:
        log.exception("Image conversion error")

    # Build structured fields and optional text
    if comment:
        title = f"Comment on: {video.title}"
        text = comment.comment
        fields: list[tuple[str, str]] = []
        if comment.timecode:
            fields.append(("Timecode", comment.timecode))
        fields.append(("By", comment.commenter))
    else:
        title = video.title
        text = None
        fields = []
        dur = humanize_duration(video.duration)
        if dur:
            fields.append(("Duration", dur))
        fields.append(("Owner", video.owner))
        if video.added_time:
            fields.append(("Added", video.added_time))

    return UnfurlResult(
        title=title,
        fields=fields,
        text=text,
        image_bytes=image_bytes,
        image_filename=image_filename,
        image_source_path=image_source_path,
    )
