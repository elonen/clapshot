"""SQLite lookups against Clapshot's database."""

import sqlite3
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path


def _utc_to_local(ts: str | None) -> str | None:
    """Convert a UTC timestamp string from SQLite to local time, date only."""
    if not ts:
        return None
    try:
        dt = datetime.fromisoformat(ts).replace(tzinfo=timezone.utc)
        return dt.astimezone().strftime("%Y-%m-%d")
    except ValueError:
        return ts


@dataclass
class MediaFileRow:
    id: str
    title: str
    duration: float | None
    added_time: str | None
    owner: str


@dataclass
class CommentRow:
    id: int
    media_file_id: str
    comment: str
    timecode: str | None
    drawing: str | None
    commenter: str


def open_db(path: Path) -> sqlite3.Connection:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    return conn


def get_media_file(conn: sqlite3.Connection, video_id: str) -> MediaFileRow | None:
    row = conn.execute("""
        SELECT m.id, m.title, m.orig_filename, m.duration, m.added_time, m.user_id,
               u.name AS user_name
        FROM media_files m
        LEFT JOIN users u ON m.user_id = u.id
        WHERE m.id = ?
    """, (video_id,)).fetchone()
    if not row:
        return None
    return MediaFileRow(
        id=row["id"],
        title=row["title"] or row["orig_filename"] or "Untitled",
        duration=row["duration"],
        added_time=_utc_to_local(row["added_time"]),
        owner=row["user_name"] or row["user_id"] or "n/a",
    )


def get_comment(conn: sqlite3.Connection, comment_id: str) -> CommentRow | None:
    row = conn.execute("""
        SELECT c.id, c.media_file_id, c.comment, c.timecode, c.drawing,
               c.user_id, c.username_ifnull,
               u.name AS user_name
        FROM comments c
        LEFT JOIN users u ON c.user_id = u.id
        WHERE c.id = ?
    """, (comment_id,)).fetchone()
    if not row:
        return None
    return CommentRow(
        id=row["id"],
        media_file_id=row["media_file_id"],
        comment=row["comment"],
        timecode=row["timecode"],
        drawing=row["drawing"],
        commenter=row["user_name"] or row["username_ifnull"] or row["user_id"] or "n/a",
    )
