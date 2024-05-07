-- comments table
CREATE TABLE comments_old (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    video_id VARCHAR NOT NULL,
    parent_id INTEGER,
    created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
    edited DATETIME,
    user_id VARCHAR NOT NULL,
    user_name VARCHAR NOT NULL,
    comment VARCHAR NOT NULL,
    timecode VARCHAR,
    drawing VARCHAR,
    FOREIGN KEY(video_id) REFERENCES videos (id),
    FOREIGN KEY(parent_id) REFERENCES comments (id)
);
INSERT INTO comments_old SELECT * FROM comments;
DROP TABLE comments;
ALTER TABLE comments_old RENAME COLUMN user_name TO username;
ALTER TABLE comments_old RENAME TO comments;

CREATE INDEX ix_comment_parent_id ON comments (parent_id);

-- messages table
ALTER TABLE messages RENAME COLUMN video_id TO ref_video_id;
ALTER TABLE messages RENAME COLUMN comment_id TO ref_comment_id;

CREATE TABLE messages_old (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    user_id VARCHAR NOT NULL,
    created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
    seen BOOLEAN NOT NULL,
    ref_video_id VARCHAR,
    ref_comment_id INTEGER,
    event_name VARCHAR NOT NULL,
    message VARCHAR NOT NULL,
    details VARCHAR NOT NULL,
    FOREIGN KEY(ref_comment_id) REFERENCES comments (id),
    FOREIGN KEY(ref_video_id) REFERENCES videos (id)
);
INSERT INTO messages_old SELECT * FROM messages;
DROP TABLE messages;
ALTER TABLE messages_old RENAME TO messages;

-- videos table
ALTER TABLE videos RENAME COLUMN user_id TO added_by_userid;
ALTER TABLE videos RENAME COLUMN user_name TO added_by_username;
