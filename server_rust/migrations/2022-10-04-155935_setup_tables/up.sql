CREATE TABLE videos (
       	id INTEGER NOT NULL,
       	video_hash VARCHAR NOT NULL,
       	added_by_userid VARCHAR,
       	added_by_username VARCHAR,
       	added_time DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
       	recompression_done DATETIME,
       	orig_filename VARCHAR,
       	total_frames INTEGER,
       	duration FLOAT,
       	fps VARCHAR,
       	raw_metadata_all VARCHAR,
       	PRIMARY KEY (id)
);
CREATE INDEX ix_video_added_by_userid ON videos (added_by_userid);
CREATE UNIQUE INDEX ix_video_video_hash ON videos (video_hash);

CREATE TABLE comments (
       	id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       	video_hash VARCHAR NOT NULL,
       	parent_id INTEGER,
       	created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
       	edited DATETIME,
       	user_id VARCHAR NOT NULL,
       	username VARCHAR NOT NULL,
       	comment VARCHAR NOT NULL,
       	timecode VARCHAR,
       	drawing VARCHAR,
       	FOREIGN KEY(video_hash) REFERENCES videos (video_hash),
       	FOREIGN KEY(parent_id) REFERENCES comments (id)
);
CREATE INDEX ix_comment_parent_id ON comments (parent_id);

CREATE TABLE messages (
       	id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       	user_id VARCHAR NOT NULL,
       	created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
       	seen BOOLEAN NOT NULL,
       	ref_video_hash VARCHAR,
       	ref_comment_id INTEGER,
       	event_name VARCHAR NOT NULL,
       	message VARCHAR NOT NULL,
       	details VARCHAR NOT NULL,
       	FOREIGN KEY(ref_comment_id) REFERENCES comments (id),
       	FOREIGN KEY(ref_video_hash) REFERENCES videos (video_hash)
);
