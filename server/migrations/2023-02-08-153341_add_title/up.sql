ALTER TABLE videos ADD COLUMN title VARCHAR(255);
UPDATE videos SET title=orig_filename
