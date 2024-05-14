# Clapshot Sysadmin Guide

### Building

I recommend building Clapshot using Docker for a clean environment:

1. Install and start Docker.
2. Run `make debian-docker` at the project root to build Debian packages.

For more manual approaches, see [[Development Setup]].

### Running unit and integration tests

Execute `make test` at the project root to run all tests in a Docker container. For server-specific tests, use `make test-local` within the `server` directory.

### How it operates

The server starts with command `clapshot-server`. It stays in foreground, and should therefore be started by a process manager like *systemd*.

Preferred deployment and upgrade method is to install server and client as Debian packages. Whereas `clapshot-server` is a foreground binary that is configured with command line options,
the Debian package contains a systemd service file that demonizes it, and config file `/etc/clapshot-server.conf` that is translated into the appropriate CLI options automatically.

Server should be put behind a reverse proxy in production, but
can be developed and tested without one. The server .deb package contains an example Nginx config file (`/usr/share/doc/clapshot-server/examples/`) that

 1. reverse proxies the server API (websocket),
 2. serves out frontend files (.html .js .css),
 3. serves uploaded video files from `videos/` directory, and
 4. contains examples on how to add HTTPS and authentication

While the server uses mostly Websocket, there's a `/api/health` endpoint that can be used for monitoring. It returns 200 OK if the server is running.

### Database upgrades

Some releases require database migrations. If you're upgrading from a previous version, **make a backup of your database** (`clapshot.sqlite`) and then either add line `migrate = true` to `/etc/clapshot-server.conf` (Debian package) or use `--migrate` option when running the server manually.

Once the server is started with migrate enabled, it will run database migrations on startup. After that, you can remove the `migrate` option and restart the server.

Running the server without migrations enabled will detect that the database is out of date, log an error and exit.

### Advanced Authentication

Clapshot server itself contains no authentication code. Instead, it trusts
HTTP server (reverse proxy) to take care of that and to pass authenticated user ID
and username in request headers. This is exactly what the basic auth / htadmin demo
above does, too:

 - `X-Remote-User-Id` / `X_Remote_User_Id` / `HTTP_X_REMOTE_USER_ID` – Authenticated user's ID (e.g. "alice.brown")
 - `X-Remote-User-Name` / `X_Remote_User_Name` / `HTTP_X_REMOTE_USER_NAME` – Display name for user (e.g. "Alice Brown")
 - `X-Remote-User-Is-Admin` / `X_Remote_User_Is_Admin` / `HTTP_X_REMOTE_USER_IS_ADMIN` – If set to "1" or "true", user is a Clapshot admin

Most modern real-world deployments will likely use some more advanced authentication mechanism, such as OAuth, Kerberos etc, but htadmin is a good starting point.

See [clapshot+htadmin.nginx.conf](client/debian/additional_files/clapshot+htadmin.nginx.conf) (Nginx config example) and [Dockerfile.demo](Dockerfile.demo) +
[docker-entry_htadmin.sh](test/docker-entry_htadmin.sh) for details on how the integration works.

Authorization is also supposed to be handled on web server, at least for now.
See for example https://github.com/elonen/ldap_authz_proxy on how to authorize users against Active Directory/LDAP groups using Nginx. I wrote it to complement Nginx spnego authn, which uses Kerberos and thus doesn't really have a concept of groups.
If you want to use Kerberos, you may also want to check out https://github.com/elonen/debian-nginx-spnego
for .deb packages.

There are currently no demos for any of these more advanced auths (`vouch-proxy` example for Okta, Google etc. would be especially welcome, if you want to contribute!).
