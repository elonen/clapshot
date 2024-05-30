-- Previous migrations (before fixing) were missing users from messages table.
-- This adds them, if necessary, to heal foreign key constraints.

INSERT OR IGNORE INTO users (id, name)
SELECT DISTINCT user_id, user_id FROM media_files WHERE user_id IS NOT NULL;

INSERT OR IGNORE INTO users (id, name)
SELECT DISTINCT user_id, COALESCE(username_ifnull, user_id) FROM comments WHERE user_id IS NOT NULL;

INSERT OR IGNORE INTO users (id, name)
SELECT DISTINCT user_id, user_id FROM messages WHERE user_id IS NOT NULL;
