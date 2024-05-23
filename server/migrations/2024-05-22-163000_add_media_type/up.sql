PRAGMA foreign_keys=OFF;

-- Step 1: Create the media_types table
CREATE TABLE media_types (
    id VARCHAR(32) NOT NULL PRIMARY KEY
);

-- Step 2: Insert the supported media types
INSERT INTO media_types (id) VALUES ('video'), ('audio'), ('image');

-- Step 3: Add media_type column
ALTER TABLE videos ADD COLUMN media_type VARCHAR(32) DEFAULT NULL REFERENCES media_types (id) ON DELETE RESTRICT ON UPDATE CASCADE;
UPDATE videos SET media_type = 'video';

-- Step 4: Rename videos table
ALTER TABLE videos RENAME TO media_files;

-- Step 5: Rename video_id column to media_file_id in all tables
ALTER TABLE comments RENAME COLUMN video_id TO media_file_id;
DROP INDEX IF EXISTS ix_comments_video_id;
CREATE INDEX IF NOT EXISTS ix_comments_media_file_id ON comments (media_file_id);

ALTER TABLE messages RENAME COLUMN video_id TO media_file_id;
DROP INDEX IF EXISTS ix_messages_video_id;
CREATE INDEX IF NOT EXISTS ix_messages_media_file_id ON messages (media_file_id);

PRAGMA foreign_keys=ON;
