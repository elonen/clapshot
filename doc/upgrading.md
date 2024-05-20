# Upgrading Clapshot to a new release

These instruction are for basic .deb -based deployments, adapt as necessary for custom ones.

1. Stop the server, `systemctl stop clapshot-server`
2. Install the new packages: `dpkg -i clapshot-*.deb`
3. Compare you current configuration (`/etc/clapshot-server.conf`) to the latest example at `/usr/share/doc/clapshot-server/examples/clapshot-server.conf`. Edit as necessary.
4. Make sure `migration = true` in the config file.
5. Start the server, `systemctl start clapshot-server`
6. Check that it started, `systemctl status clapshot-server`
7. Review server log at `/var/log/clapshot.log`. If the server or organizer applied any **database migrations**, they are mention in the log, and a backup `.tar.gz` will be present next to you database (usually, `/mnt/clapshot-data/data/clapshot.sqlite`).
8. If the server didn't start properly, set `debug = true` in `/etc/clapshot-server.conf`, and start again. This will increase log verbosity level, to give you more clues on what went wrong.

### Notes

 - Make sure to fully reload the Client page on a browers if Client package was upgraded. You might otherwise see unexpected behavior.
 - The basic_folders Organizer plugin doesn't have its own systemd entry (it's executed by the Server), so you don't need to `systemct stop/start` it. It also piggybacks the server when doing migrations.

If you find this migration guide lacking, please contribute corrections and additions on the [Clapshot's GitHub page](https://github.com/elonen/clapshot).

## Recovering lost comments from 0.5.6 -> 0.6.0 migration

The database migration script on release 0.6.0 had a bug that would lose existing comments.
Here's the procedure to restore them, in case you got burnt by this before the broken release was pulled:

1. `cd /mnt/clapshot-data/` (or where ever your `clapshot.sqlite` is)
2. Check if the DB has any comments: `sqlite3 clapshot.sqlite "select count(*) from comments;"` (if this returns "0" but you previously had comments, continue to the next step to restore them.)
3. Locate the latest sqlite backup: `ls clapshot.backup-*`. Clapshot server makes backups automatically before all migrations. It should look something like: `clapshot.backup-2024-05-20T12_34_56.tar.gz`
4. Unpack the backup to a temp dir: `mkdir db-restore-temp && cd db-restore-temp && tar xvfz ../clapshot.backup-2024-05-20T12_34_56.tar.gz` (replace the .tar.gz with the latest you have)
5. Dump comments from the backup into a text file: `sqlite3 ./clapshot.sqlite ".dump --data-only comments" > comments-restore.sql`
6. Check that the comments are there: `less comments-restore.sql`
7. Insert them back to current DB: `sqlite3 ../clapshot.sqlite ".read ./comments-restore.sql"`
8. Verify: `sqlite3 ../clapshot.sqlite "select count(*) from comments;"` (this should now return the correct number of comments)

You can now remove the temp directory `db-restore-temp`, no need to restart the server.
