-- This migration adds ON UPDATE and ON DELETE cascade rules to the comments and messages.

-- comments table
CREATE TABLE comments_new (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    video_id VARCHAR NOT NULL REFERENCES videos(id) ON UPDATE CASCADE ON DELETE CASCADE,
    parent_id INTEGER REFERENCES comments(id) ON UPDATE CASCADE ON DELETE CASCADE,
    created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
    edited DATETIME,
    user_id VARCHAR NOT NULL,
    username VARCHAR NOT NULL,
    comment VARCHAR NOT NULL,
    timecode VARCHAR,
    drawing VARCHAR
);
INSERT INTO comments_new SELECT * FROM comments;
DROP TABLE comments;
ALTER TABLE comments_new RENAME COLUMN username TO user_name;
ALTER TABLE comments_new RENAME TO comments;

-- messages table
ALTER TABLE messages RENAME COLUMN ref_video_id TO video_id;
ALTER TABLE messages RENAME COLUMN ref_comment_id TO comment_id;

CREATE TABLE messages_new (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    user_id VARCHAR NOT NULL,
    created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
    seen BOOLEAN NOT NULL,
    video_id VARCHAR REFERENCES videos (id) ON UPDATE CASCADE,  -- keep messages about videos even if it's deleted, so no ON DELETE CASCADE
    comment_id INTEGER REFERENCES comments (id) ON UPDATE CASCADE ON DELETE CASCADE,
    event_name VARCHAR NOT NULL,
    message VARCHAR NOT NULL,
    details VARCHAR NOT NULL
);
INSERT INTO messages_new SELECT * FROM messages;
DROP TABLE messages;
ALTER TABLE messages_new RENAME TO messages;

-- videos table
ALTER TABLE videos RENAME COLUMN added_by_userid TO user_id;
ALTER TABLE videos RENAME COLUMN added_by_username TO user_name;
