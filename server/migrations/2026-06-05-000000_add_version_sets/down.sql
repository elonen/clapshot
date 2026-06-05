-- SQLite does not support DROP COLUMN before 3.35, but Diesel targets older versions.
-- Recreate media_files without the version_set_id column.

PRAGMA foreign_keys = OFF;

CREATE TABLE media_files_backup (
    id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    media_type TEXT REFERENCES media_types(id),
    added_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    recompression_done TIMESTAMP,
    thumbs_done TIMESTAMP,
    has_thumbnail BOOLEAN,
    thumb_sheet_cols INTEGER,
    thumb_sheet_rows INTEGER,
    orig_filename VARCHAR,
    title VARCHAR,
    total_frames INTEGER,
    duration FLOAT,
    fps VARCHAR,
    raw_metadata_all TEXT,
    default_subtitle_id INTEGER REFERENCES subtitles(id) ON DELETE SET NULL
);

INSERT INTO media_files_backup SELECT
    id, user_id, media_type, added_time, recompression_done, thumbs_done,
    has_thumbnail, thumb_sheet_cols, thumb_sheet_rows, orig_filename, title,
    total_frames, duration, fps, raw_metadata_all, default_subtitle_id
FROM media_files;

DROP TABLE media_files;
ALTER TABLE media_files_backup RENAME TO media_files;

DROP INDEX IF EXISTS media_files_version_set_id;
DROP TABLE IF EXISTS version_sets;

PRAGMA foreign_keys = ON;
