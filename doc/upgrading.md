# Upgrading Clapshot to a new release

These instructions are for basic .deb-based deployments, adapt as necessary for custom ones.

1. Stop the server, `systemctl stop clapshot-server`
2. Install the new packages: `dpkg -i clapshot-*.deb`
3. Compare your current configuration (`/etc/clapshot-server.conf`) to the latest example at `/usr/share/doc/clapshot-server/examples/clapshot-server.conf`. Edit as necessary.
4. Make sure `migration = true` in the config file.
5. Start the server, `systemctl start clapshot-server`
6. Check that it started, `systemctl status clapshot-server`
7. Review server log at `/var/log/clapshot.log`. If the server or organizer applied any **database migrations**, they are mentioned in the log, and a backup `.tar.gz` will be present next to your database (usually, `/mnt/clapshot-data/data/clapshot.sqlite`).
8. If the server didn't start properly, set `debug = true` in `/etc/clapshot-server.conf`, and start again. This will increase log verbosity level, to give you more clues on what went wrong.

### Notes

 - Make sure to fully reload the Client page in a browser if Client package was upgraded. You might otherwise see unexpected behavior.
 - The basic_folders Organizer plugin doesn't have its own systemd entry (it's executed by the Server), so you don't need to `systemctl stop/start` it. It also piggybacks the server when doing migrations.

If you find this migration guide lacking, please contribute corrections and additions on the [Clapshot's GitHub page](https://github.com/elonen/clapshot).

## Migrating the demo/installer auth from htadmin to htwicket

Starting with the htwicket-based demo image (`elonen/clapshot:latest-demo-htwicket`) and
the updated `extras/install-clapshot-deb.sh`, the PHP `htadmin` + HTTP Basic Auth example
has been replaced by [htwicket](https://github.com/elonen/htwicket): a small nginx
`auth_request` gateway that manages the same `/var/www/.htpasswd`, but adds a real login
form, working cookie logout, and a web user-management UI. If you used a custom
authentication setup (OAuth, LDAP, Kerberos, ...) none of this affects you — the
`X-Remote-User-*` header contract is unchanged.

For deployments based on the old htadmin example:

1. **Passwords carry over automatically.** htwicket reads the same `/var/www/.htpasswd`
   and verifies all the legacy hash formats (DES crypt, `$apr1$`, `$1$`, `$5$`/`$6$`,
   bcrypt). No reset, no import, no user re-creation. The new *random admin password*
   behavior only applies to **fresh** installs (no existing `.htpasswd`); your existing
   file is never touched on upgrade.

2. **The separate `htadmin` management login is gone — and it does not migrate.** htadmin
   kept its own admin account (`admin_user` / `admin_pwd_hash`) in `config.ini`, *outside*
   `.htpasswd`, so there is nothing to import. htwicket has no separate management account:
   the user-management UI at **`/htwicket/admin`** is gated by a `[superadmins]` rule over
   your *real* `.htpasswd` users. By default, **the user named `admin` doubles as the
   htwicket superadmin** (plus anyone with `is_admin = true` in the sidecar). So just log in
   as your existing `admin`.
   - No `admin` user, or forgot the password? Reset it on the CLI:
     `sudo -u www-data htwicket user passwd admin --random` (prints a new password), or
     `sudo -u www-data htwicket user add admin` to create one.
   - Superadmin named something other than `admin`? Edit `[superadmins].expr` in
     `/etc/htwicket.toml`, or add `[users."NAME"] is_admin = true` to `/var/www/.htwicket.toml`.

3. **Switch the nginx site** from `clapshot+htadmin.nginx.conf` to
   `clapshot+htwicket.nginx.conf` and remove the old one (two `default_server` blocks
   prevent nginx from starting). Install the `htwicket` package and
   `systemctl enable --now htwicket`. The `extras/install-clapshot-deb.sh` script does all
   of this for you (and removes the stale htadmin config + `/var/www/htadmin` on re-run).
   You no longer need `php-fpm`.

4. **Client logout.** Set `"user_menu_show_basic_auth_logout": false` and add a
   `{"label":"Logout","type":"url","data":"/htwicket/logout"}` item to
   `user_menu_extra_items` in `/etc/clapshot_client.conf`, so the menu offers htwicket's
   real logout instead of the old (now removed) Basic-Auth `/logout` hack. The installer
   does this automatically.

5. **Cookie scheme gotcha (important).** htwicket marks its session cookie `Secure` over
   HTTPS. If your public URL is `https://`, keep `insecure_cookies = false`; if it is plain
   `http://`, set `insecure_cookies = true`. A mismatch makes login *silently* fail (the
   browser drops the cookie). The installer and demo entrypoint derive this from the URL
   scheme automatically.

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
