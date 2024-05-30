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

-- Step 4: Rename 'videos' table to 'media_files'
ALTER TABLE videos RENAME TO media_files;
CREATE INDEX IF NOT EXISTS ix_media_files_thumbs_done ON media_files (thumbs_done);

-- Step 5: Recreate comments table with 'media_file_id' instead of 'video_id'
DROP INDEX IF EXISTS ix_comments_user_id;
DROP INDEX IF EXISTS ix_comments_parent_id;

CREATE TABLE IF NOT EXISTS "comments_new" (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    video_id VARCHAR NOT NULL REFERENCES media_files(id) ON UPDATE CASCADE ON DELETE CASCADE,
    parent_id INTEGER REFERENCES comments(id) ON UPDATE CASCADE ON DELETE CASCADE,
    created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
    edited DATETIME,
    user_id user_id VARCHAR(255) NULL REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    username_ifnull VARCHAR(255) NOT NULL,
    comment VARCHAR NOT NULL,
    timecode VARCHAR,
    drawing VARCHAR
);

INSERT INTO comments_new SELECT * FROM comments;
ALTER TABLE comments_new RENAME COLUMN video_id TO media_file_id;

DROP TABLE comments;
ALTER TABLE comments_new RENAME TO comments;

CREATE INDEX IF NOT EXISTS ix_comments_user_id ON comments (user_id);
CREATE INDEX IF NOT EXISTS ix_comments_parent_id ON comments (parent_id);
CREATE INDEX IF NOT EXISTS ix_comments_media_file_id ON comments (media_file_id);

-- Step 6: Rename 'video_id' column to 'media_file_id' in other tables
ALTER TABLE messages RENAME COLUMN video_id TO media_file_id;   -- Not a foreign key, so no need to recreate the table
DROP INDEX IF EXISTS ix_messages_video_id;
CREATE INDEX IF NOT EXISTS ix_messages_media_file_id ON messages (media_file_id);
