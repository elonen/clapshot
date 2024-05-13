-- Harmonize names, add some cascade rules and indexes

-- comments table (add cascade rules, rename fields)
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

-- videos table
ALTER TABLE videos RENAME COLUMN added_by_userid TO user_id;
ALTER TABLE videos RENAME COLUMN added_by_username TO user_name;

-- add indexes
CREATE INDEX 'comments_video_id' ON 'comments'('video_id');
CREATE INDEX 'comments_parent_id' ON 'comments'('parent_id');

CREATE INDEX 'messages_user_id' ON 'messages'('user_id');
CREATE INDEX 'messages_comment_id' ON 'messages'('comment_id');
CREATE INDEX 'messages_video_id' ON 'messages'('video_id');
