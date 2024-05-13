DROP INDEX IF EXISTS ix_videos_video_hash;
DROP INDEX IF EXISTS ix_videos_added_by_userid;

ALTER TABLE videos RENAME COLUMN id TO old_id;
ALTER TABLE videos RENAME COLUMN video_hash TO id;  -- converts foreign keys also

ALTER TABLE comments RENAME COLUMN video_hash TO video_id;
ALTER TABLE messages RENAME COLUMN ref_video_hash TO ref_video_id;

-- Split 'thumb_sheet_dims' (e.g. string "8x10") into 'thumb_sheet_cols' and 'thumb_sheet_rows' (e.g. 8 and 10)
ALTER TABLE videos ADD COLUMN thumb_sheet_cols INTEGER;
ALTER TABLE videos ADD COLUMN thumb_sheet_rows INTEGER;
UPDATE videos SET thumb_sheet_cols = CAST(SUBSTR(thumb_sheet_dims, 1, INSTR(thumb_sheet_dims, 'x') - 1) AS INTEGER);
UPDATE videos SET thumb_sheet_rows = CAST(SUBSTR(thumb_sheet_dims, INSTR(thumb_sheet_dims, 'x') + 1) AS INTEGER);
ALTER TABLE videos DROP COLUMN thumb_sheet_dims;


-- Drop old_id primary key,
-- parse thumb_sheet_dims into thumb_sheet_cols and thumb_sheet_rows

CREATE TABLE videos_new (
        old_id VARCHAR, -- keep old_id for the copy, delete after
        id VARCHAR NOT NULL PRIMARY KEY,
        added_by_userid VARCHAR NOT NULL,
        added_by_username VARCHAR NOT NULL,
        added_time DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
        recompression_done DATETIME,
        orig_filename VARCHAR,
        total_frames INTEGER,
        duration FLOAT,
        fps VARCHAR,
        raw_metadata_all VARCHAR,
        title VARCHAR(255),
        thumb_sheet_cols INTEGER,
        thumb_sheet_rows INTEGER
);

INSERT INTO videos_new SELECT * FROM videos;
ALTER TABLE videos_new DROP COLUMN old_id;

DROP TABLE videos;
ALTER TABLE videos_new RENAME TO videos;
CREATE INDEX ix_video_added_by_userid ON videos (added_by_userid);
