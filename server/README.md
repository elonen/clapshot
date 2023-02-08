# Clapshot server

This is the server component for Clapshot - a self-hosted collaborative
video review tool. See github page for an overview of the whole tool.

The server is a HTTP daemon running behind a reverse proxy, and
controlled by systemd or some other process manager.

Frontend is provided in another package, and consists of static
HTML, JS and CSS files.

If you have installed it in a Debian system, configure it
by editing `/etc/clapshot-server.conf`.
Otherwise, try `clapshot-server --help` for startup options.
 