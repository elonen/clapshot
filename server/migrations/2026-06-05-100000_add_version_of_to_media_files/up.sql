ALTER TABLE media_files ADD COLUMN version_of TEXT NULL REFERENCES media_files(id) ON DELETE SET NULL;
CREATE INDEX idx_media_files_version_of ON media_files(version_of);
