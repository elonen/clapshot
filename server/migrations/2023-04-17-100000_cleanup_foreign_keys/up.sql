PRAGMA foreign_keys = OFF;

-- Convert ref_video_hash and ref_comment_id to non-foreign keys.
-- They are supposed to remain in messages even if the video or comment is deleted.

CREATE TABLE messages_new (
       	id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       	user_id VARCHAR NOT NULL,
       	created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
       	seen BOOLEAN NOT NULL,
       	ref_video_hash VARCHAR,
       	ref_comment_id INTEGER,
       	event_name VARCHAR NOT NULL,
       	message VARCHAR NOT NULL,
       	details VARCHAR NOT NULL
);

INSERT INTO messages_new SELECT * FROM messages;
DROP TABLE messages;
ALTER TABLE messages_new RENAME TO messages;

PRAGMA foreign_keys = ON;
