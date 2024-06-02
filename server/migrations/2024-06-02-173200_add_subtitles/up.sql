-- Step 1: Add 'subtitles' table
CREATE TABLE IF NOT EXISTS "subtitles" (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    media_file_id VARCHAR(255) NOT NULL REFERENCES media_files(id) ON UPDATE CASCADE ON DELETE CASCADE,
    title VARCHAR NOT NULL,    -- Title of the subtitle (.e.g "English CC", "SFX cues", "Spanish Auto-translated" etc)
    language_code VARCHAR(10),
    filename VARCHAR, -- Filename of WebVTT file, either transcoded or original
    orig_filename VARCHAR NOT NULL, -- Original file name of the subtitle, regardless of format
    added_time DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
    time_offset FLOAT DEFAULT 0 NOT NULL, -- Added to subtitle timecode before displaying on video player. Can be negative.
    UNIQUE (media_file_id, filename)
);

CREATE INDEX ix_subtitles_media_file_id ON subtitles (media_file_id);


-- Step 2: Integrate with 'comments'

ALTER TABLE comments ADD COLUMN subtitle_id INTEGER REFERENCES subtitles(id) ON UPDATE CASCADE;
ALTER TABLE comments ADD COLUMN subtitle_filename_ifnull VARCHAR; -- Placeholder to record the original subtitle filename if it is deleted

CREATE TRIGGER tr_comments_set_filename_on_subtitle_delete AFTER DELETE ON subtitles
FOR EACH ROW BEGIN
    UPDATE comments SET subtitle_filename_ifnull = OLD.orig_filename WHERE subtitle_id = OLD.id;
END;

CREATE INDEX ix_comments_subtitle_id ON comments (subtitle_id);


-- Step 3: Integrate with 'messages'

ALTER TABLE messages ADD COLUMN subtitle_id INTEGER;    -- Retain even if subtitle is deleted, hence no foreign key constraint


-- Step 4: Integrate with 'media_files'

ALTER TABLE media_files ADD COLUMN default_subtitle_id INTEGER DEFAULT NULL REFERENCES subtitles(id) ON UPDATE CASCADE ON DELETE SET NULL;
