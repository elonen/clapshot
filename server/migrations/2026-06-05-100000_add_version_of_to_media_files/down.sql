DROP INDEX IF EXISTS idx_media_files_version_of;
-- SQLite ne supporte pas DROP COLUMN directement, on recrée la table
CREATE TABLE media_files_backup AS SELECT id, user_id, media_type, added_time, recompression_done, thumbs_done, has_thumbnail, thumb_sheet_cols, thumb_sheet_rows, orig_filename, title, total_frames, duration, fps, raw_metadata_all, default_subtitle_id FROM media_files;
DROP TABLE media_files;
ALTER TABLE media_files_backup RENAME TO media_files;
