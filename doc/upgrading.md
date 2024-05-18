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
