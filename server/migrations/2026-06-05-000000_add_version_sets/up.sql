-- Version sets: a group of media files that are different versions of the same content.
-- Each member keeps its own comments. Users can navigate between versions from the player.

CREATE TABLE version_sets (
    id      TEXT NOT NULL PRIMARY KEY,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE media_files ADD COLUMN version_set_id TEXT REFERENCES version_sets(id) ON DELETE SET NULL;

CREATE INDEX media_files_version_set_id ON media_files (version_set_id);
