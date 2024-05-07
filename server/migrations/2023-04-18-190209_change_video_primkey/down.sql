-- Rename id back to video_hash and revert other column changes
ALTER TABLE videos RENAME COLUMN id TO video_hash;
ALTER TABLE comments RENAME COLUMN video_id TO video_hash;
ALTER TABLE messages RENAME COLUMN ref_video_id TO ref_video_hash;

-- Create a new table with the original structure
CREATE TABLE old_videos (
        id INTEGER NOT NULL,
        video_hash VARCHAR NOT NULL,
        added_by_userid VARCHAR,
        added_by_username VARCHAR,
        added_time DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
        recompression_done DATETIME,
        orig_filename VARCHAR,
        total_frames INTEGER,
        duration FLOAT,
        fps VARCHAR,
        raw_metadata_all VARCHAR,
        title VARCHAR(255),
        thumb_sheet_dims VARCHAR(255),
        PRIMARY KEY (id)
);

-- Insert data from the current videos table to the old_videos table
INSERT INTO old_videos (
    id,
    video_hash,
    added_by_userid,
    added_by_username,
    added_time,
    recompression_done,
    orig_filename,
    total_frames,
    duration,
    fps,
    raw_metadata_all,
    title,
    thumb_sheet_dims
)
SELECT
    ROWID,
    video_hash,
    added_by_userid,
    added_by_username,
    added_time,
    recompression_done,
    orig_filename,
    total_frames,
    duration,
    fps,
    raw_metadata_all,
    title,
    thumb_sheet_cols || 'x' || thumb_sheet_rows
FROM
    videos;

-- Drop the current videos table and rename the old_videos table back to videos
DROP TABLE videos;
ALTER TABLE old_videos RENAME TO videos;
CREATE INDEX ix_video_added_by_userid ON videos (added_by_userid);
