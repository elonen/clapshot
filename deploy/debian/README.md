# Clapshot on a VM (Debian package + systemd)

Install Clapshot directly on a Debian/Ubuntu host (or VM), without containers. Clapshot
runs as systemd services behind nginx, with the data on the host filesystem.

This is the right choice when you already manage a VM and prefer packages to containers.

## Install

Execute or read and follow along manually:

```sh
./install-clapshot-deb.sh
```

The script installs the `clapshot-server`, `clapshot-organizer-basic-folders`, and
`clapshot-client` packages, sets up nginx, and points data at `/mnt/clapshot-data`.

## Authentication & HTTPS

Same model as everywhere else: nginx sets the `X-Remote-User-*` headers from whatever
auth you configure, and TLS is terminated by the remote proxy.

- **Auth:** see [Advanced Authentication](../../doc/sysadmin-guide.md#advanced-authentication).
- **HTTPS:** terminate TLS in nginx (Let's Encrypt via certbot, or your own certs).

See the [sysadmin guide](../../doc/sysadmin-guide.md) for full configuration.
