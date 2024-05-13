-- This migration adds 'users' table and normalizes existing 'user_id' and 'user_name'
-- columns against it, with foreign keys.
--
-- The idea is to enhance data integrity and to help organizers
-- to list users more easily. Comment table is an exception:
-- 'user_id' fk is nullable, to keep comments even if the user is deleted.


-- Drop existing indexes that will interfere with column modifications
DROP INDEX IF EXISTS ix_video_added_by_userid;
DROP INDEX IF EXISTS messages_user_id;

-- Step 1: Create the users table
CREATE TABLE users (
    id VARCHAR(255) NOT NULL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    created DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Step 2: Populate the users table from existing data
-- Insert distinct users from videos
INSERT INTO users (id, name)
SELECT DISTINCT user_id, user_name FROM videos WHERE user_id IS NOT NULL AND user_name IS NOT NULL;

-- Insert distinct users from comments (where not already included)
INSERT OR IGNORE INTO users (id, name)
SELECT DISTINCT user_id, user_name FROM comments WHERE user_name IS NOT NULL;


-- Step 3: Modify the videos table
-- Remove the `user_name` column, and make `user_id` a foreign key column
ALTER TABLE videos DROP COLUMN user_name;
CREATE TABLE videos_new (
        id VARCHAR NOT NULL,
        user_id VARCHAR(255) NOT NULL REFERENCES users (id) ON DELETE RESTRICT ON UPDATE CASCADE,
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
INSERT INTO videos_new SELECT * FROM videos;
DROP TABLE videos;
ALTER TABLE videos_new RENAME TO videos;

CREATE INDEX ix_video_user_id ON videos (user_id);
CREATE INDEX ix_video_added_time ON videos (added_time);


-- Step 4: Modify the comments table
-- Rename user_name into username_ifnull, make current user_id a NULLABLE foreign key
-- Unlike videos, this allows comments without login, and to keep comments even if the user is deleted

ALTER TABLE comments RENAME COLUMN user_name TO username_ifnull;
CREATE TABLE comments_new (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    video_id VARCHAR NOT NULL REFERENCES videos(id) ON UPDATE CASCADE ON DELETE CASCADE,
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
DROP TABLE comments;
ALTER TABLE comments_new RENAME TO comments;

CREATE INDEX ix_comments_user_id ON comments (user_id);
CREATE INDEX ix_comments_parent_id ON comments (parent_id);

-- When users are deleted, update their name to all their comments from now on
CREATE TRIGGER tr_comments_set_username_on_user_delete AFTER DELETE ON users
BEGIN
    UPDATE comments SET username_ifnull = OLD.name WHERE user_id = OLD.id;
END;


-- Step 5: Modify the messages table
-- Make user_id a foreign key

CREATE TABLE messages_new (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    user_id VARCHAR(255) NOT NULL REFERENCES users (id) ON DELETE CASCADE ON UPDATE CASCADE,
    created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
    seen BOOLEAN NOT NULL,
    video_id VARCHAR,  -- keep messages about videos even if it's deleted, so no foreign key
    comment_id INTEGER REFERENCES comments (id) ON UPDATE CASCADE ON DELETE CASCADE,
    event_name VARCHAR NOT NULL,
    message VARCHAR NOT NULL,
    details VARCHAR NOT NULL
);
INSERT INTO messages_new SELECT * FROM messages;
DROP TABLE messages;
ALTER TABLE messages_new RENAME TO messages;

CREATE INDEX ix_messages_user_id ON messages (user_id);
CREATE INDEX ix_messages_comment_id ON messages (comment_id);
CREATE INDEX ix_messages_video_id ON messages (video_id);
