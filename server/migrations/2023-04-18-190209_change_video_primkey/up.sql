ALTER TABLE videos RENAME COLUMN id TO old_id;
ALTER TABLE videos RENAME COLUMN video_hash TO id;

ALTER TABLE comments RENAME COLUMN video_hash TO video_id;
ALTER TABLE messages RENAME COLUMN ref_video_hash TO ref_video_id;

-- Drop old_id primary key,
-- parse thumb_sheet_dims into thumb_sheet_cols and thumb_sheet_rows

CREATE TABLE new_videos (
        id VARCHAR NOT NULL,
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
        thumb_sheet_cols INTEGER,
        thumb_sheet_rows INTEGER,
        PRIMARY KEY (id)
);

INSERT INTO new_videos (
    id,
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
    thumb_sheet_cols,
    thumb_sheet_rows
)
SELECT
    id,
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
    CAST(SUBSTR(thumb_sheet_dims, 1, INSTR(thumb_sheet_dims, 'x') - 1) AS INTEGER) AS thumb_sheet_cols,
    CAST(SUBSTR(thumb_sheet_dims, INSTR(thumb_sheet_dims, 'x') + 1) AS INTEGER) AS thumb_sheet_rows
FROM
    videos;

DROP TABLE videos;
ALTER TABLE new_videos RENAME TO videos;
CREATE INDEX ix_video_added_by_userid ON videos (added_by_userid);
