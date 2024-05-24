PRAGMA foreign_keys=OFF;

-- Step 1: Create the media_types table
CREATE TABLE media_types (
    id VARCHAR(32) NOT NULL PRIMARY KEY
);

-- Step 2: Insert the supported media types
INSERT INTO media_types (id) VALUES ('video'), ('audio'), ('image');

-- Step 3: Add 'media_type', 'thumbs_done', 'has_thumbnail'
ALTER TABLE videos ADD COLUMN media_type VARCHAR(32) DEFAULT NULL REFERENCES media_types (id) ON DELETE RESTRICT ON UPDATE CASCADE;
ALTER TABLE videos ADD COLUMN thumbs_done DATETIME DEFAULT NULL;
ALTER TABLE videos ADD COLUMN has_thumbnail BOOLEAN DEFAULT NULL;

UPDATE videos SET media_type = 'video';
UPDATE videos SET thumbs_done = COALESCE(recompression_done, added_time) WHERE thumb_sheet_cols IS NOT NULL; -- For existing videos, assume thumb is done if thumb_sheet_cols is set
UPDATE videos SET has_thumbnail = true WHERE thumb_sheet_cols IS NOT NULL;

-- Step 4: Rename videos table
ALTER TABLE videos RENAME TO media_files;

-- Step 5: Rename video_id column to media_file_id in all tables
ALTER TABLE comments RENAME COLUMN video_id TO media_file_id;
DROP INDEX IF EXISTS ix_comments_video_id;
CREATE INDEX IF NOT EXISTS ix_comments_media_file_id ON comments (media_file_id);

ALTER TABLE messages RENAME COLUMN video_id TO media_file_id;
DROP INDEX IF EXISTS ix_messages_video_id;
CREATE INDEX IF NOT EXISTS ix_messages_media_file_id ON messages (media_file_id);

CREATE INDEX IF NOT EXISTS ix_media_files_thumbs_done ON media_files (thumbs_done);

PRAGMA foreign_keys=ON;
