"""Shared test fixtures — real SQLite DB and thumbnail files."""

import sqlite3
from pathlib import Path
import pytest
from PIL import Image


@pytest.fixture
def clapshot_env(tmp_path):
    """Create a minimal Clapshot data environment with DB + files."""
    db_path = tmp_path / "clapshot.sqlite"
    videos_dir = tmp_path / "videos"

    conn = sqlite3.connect(db_path)
    conn.executescript("""
        CREATE TABLE users (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE media_files (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            media_type TEXT,
            added_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            recompression_done TIMESTAMP,
            thumbs_done TIMESTAMP,
            has_thumbnail BOOLEAN,
            thumb_sheet_cols INTEGER,
            thumb_sheet_rows INTEGER,
            orig_filename TEXT,
            title TEXT,
            total_frames INTEGER,
            duration REAL,
            fps TEXT,
            raw_metadata_all TEXT,
            default_subtitle_id INTEGER
        );
        CREATE TABLE comments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            media_file_id TEXT NOT NULL,
            parent_id INTEGER,
            created TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            edited TIMESTAMP,
            user_id TEXT,
            username_ifnull TEXT NOT NULL DEFAULT '',
            comment TEXT NOT NULL,
            timecode TEXT,
            drawing TEXT,
            subtitle_id INTEGER,
            subtitle_filename_ifnull TEXT
        );
        INSERT INTO users (id, name) VALUES ('alice', 'Alice Anderson');
        INSERT INTO users (id, name) VALUES ('bob', 'Bob Builder');
        INSERT INTO media_files (id, user_id, title, orig_filename, duration, thumbs_done, has_thumbnail)
            VALUES ('a1b2c3d4', 'alice', 'Hero Shot v3', 'hero_shot_v3.mp4', 125.7, '2025-01-01', 1);
        INSERT INTO media_files (id, user_id, orig_filename, duration, thumbs_done, has_thumbnail)
            VALUES ('deadbeef', 'bob', 'untitled.mov', 3661.0, '2025-01-01', 1);
        INSERT INTO comments (id, media_file_id, user_id, username_ifnull, comment, timecode, drawing)
            VALUES (42, 'a1b2c3d4', 'bob', 'Bob Builder', 'Fix the lighting here', '00:01:05.500', 'abcdef12.webp');
        INSERT INTO comments (id, media_file_id, user_id, username_ifnull, comment, timecode)
            VALUES (43, 'a1b2c3d4', 'alice', 'Alice Anderson', 'Looks good!', '00:00:30.000');
        INSERT INTO comments (id, media_file_id, user_id, username_ifnull, comment, timecode, drawing)
            VALUES (99, 'a1b2c3d4', 'bob', 'Bob Builder', 'Malicious', '00:00:01.000', '../../../etc/passwd');
    """)
    conn.close()

    # Create thumbnail files (1x1 WebP)
    for vid in ("a1b2c3d4", "deadbeef"):
        thumb_dir = videos_dir / vid / "thumbs"
        thumb_dir.mkdir(parents=True)
        img = Image.new("RGB", (160, 90), color=(100, 100, 100))
        img.save(thumb_dir / "thumb.webp")

    # Create drawing file
    drawing_dir = videos_dir / "a1b2c3d4" / "drawings"
    drawing_dir.mkdir(parents=True)
    img = Image.new("RGB", (320, 180), color=(200, 50, 50))
    img.save(drawing_dir / "abcdef12.webp")

    return {"db_path": db_path, "videos_dir": videos_dir}
