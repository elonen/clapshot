-- SQLite does not support DROP COLUMN in older versions; recreate table without email
CREATE TABLE users_backup (
    id VARCHAR(255) NOT NULL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    created DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO users_backup (id, name, created) SELECT id, name, created FROM users;
DROP TABLE users;
ALTER TABLE users_backup RENAME TO users;
