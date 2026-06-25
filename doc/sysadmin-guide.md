# Clapshot Sysadmin Guide

> **New to Clapshot?** Start with the [Quick Start Reference](quick-start-reference.md) for common deployment scenarios.
> **Having connection issues?** See the [Connection Troubleshooting Guide](connection-troubleshooting.md) for help with common deployment and connectivity problems.
> **Want to customize media processing?** See the [Transcoding and Thumbnailing Guide](transcoding.md) for configuring hardware acceleration and custom workflows.

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
can be developed and tested without one. The client .deb package contains an example Nginx config file (`/usr/share/doc/clapshot-client/examples/`) that

 1. reverse proxies the server API (websocket),
 2. serves out frontend files (.html .js .css),
 3. serves uploaded video files from `videos/` directory, and
 4. contains examples on how to add HTTPS and authentication

While the server uses mostly Websocket, there's a `/api/health` endpoint that can be used for monitoring. It returns 200 OK if the server is running.

### Database upgrades

Running a new version of Clapshot Server (and/or Organizer) will often upgrade database schemas on first start.
See [upgrading.md](upgrading.md) for details about upgrading.

### Advanced Authentication

Clapshot server itself contains no authentication code. Instead, it trusts
HTTP server (reverse proxy) to take care of that and to pass authenticated user ID
and username in request headers. This is exactly what the HTWicket demo
above does, too:

 - `X-Remote-User-Id` / `X_Remote_User_Id` / `HTTP_X_REMOTE_USER_ID` – Authenticated user's ID (e.g. "alice.brown")
 - `X-Remote-User-Name` / `X_Remote_User_Name` / `HTTP_X_REMOTE_USER_NAME` – Display name for user (e.g. "Alice Brown")
 - `X-Remote-User-Is-Admin` / `X_Remote_User_Is_Admin` / `HTTP_X_REMOTE_USER_IS_ADMIN` – If set to "1" or "true", user is a Clapshot admin
 - `X-Remote-User-Can-Upload` / `X_Remote_User_Can_Upload` / `HTTP_X_REMOTE_USER_CAN_UPLOAD` – If set to "1" or "true", user is allowed to upload media files
 - `X-Remote-Error` / `X_Remote_Error` / `HTTP_X_REMOTE_ERROR` – If set, displays authentication error message instead of normal UI

Most modern real-world deployments will likely use some more advanced authentication mechanism, such as OAuth, Kerberos etc, but [HTWicket](https://github.com/elonen/htwicket) is a good starting point. It's a small nginx `auth_request` gateway that manages an `.htpasswd` file, offers a login form with real cookie logout, and a web UI for user management. Per-user attributes (admin rights, upload permission, display name) are kept in a sidecar `.htwicket.toml` and exposed to Clapshot via `[headers.*]` CEL expressions.

See [clapshot+htwicket.nginx.conf](client/debian/additional_files/clapshot+htwicket.nginx.conf) (Nginx config example) and [Dockerfile.demo](Dockerfile.demo) +
[docker-entry_htwicket.sh](test/docker-entry_htwicket.sh) for details on how the integration works.

Authorization is also supposed to be handled on web server, at least for now.
See for example https://github.com/elonen/ldap_authz_proxy on how to authorize users against Active Directory/LDAP groups using Nginx. I wrote it to complement Nginx spnego authn, which uses Kerberos and thus doesn't really have a concept of groups.
If you want to use Kerberos, you may also want to check out https://github.com/elonen/debian-nginx-spnego
for .deb packages.

There are currently no demos for any of these more advanced auths (`vouch-proxy` example for Okta, Google etc. would be especially welcome, if you want to contribute!).

### Upload Permission Control

Clapshot's `basic_folders` Organizer supports restricting file upload permissions per user through HTTP headers. This allows integration with external authorization systems (e.g. LDAP).

#### Configuration

Upload permission is controlled via the `X-Remote-User-Can-Upload` header set by your reverse proxy. Set it "true" to allow uploads; "false" to deny.

#### HTWicket Example

In the HTWicket demo, upload permission is a per-user `can_upload` field (default `true`)
stored in the sidecar `.htwicket.toml` and turned into the `X-Remote-User-Can-Upload`
header by a CEL expression in `/etc/htwicket.toml`:

```toml
# /etc/htwicket.toml
[fields.can_upload]
type = "bool"
default = true

[headers.X-Remote-User-Can-Upload]
type = "bool"
expr = "fields.can_upload"
```

```toml
# /var/www/.htwicket.toml (sidecar) - deny uploads for user 'bob'
[users."bob"]
can_upload = false
```

If instead your reverse proxy authenticates users some other way, just have it set the
`X-Remote-User-Can-Upload` header ("true"/"false") however it sees fit.

The `demo-htwicket` Docker image includes a demo user with restricted upload permissions:

- `bob:bob123` (cannot upload files) - demonstrates upload permission restrictions

All other demo users (`admin`, `demo`, `alice`) have full upload permissions.

### Monitored Folder Ingestion

Clapshot automatically processes media files dropped into the monitored incoming folder. This system enables batch uploads and integration with external tools or workflows.

#### Configuration

The incoming folder monitoring is configured via command-line options:

- `--data-dir <path>` - Base directory containing the `incoming/` subdirectory (default: current directory)
- `--poll <seconds>` - Polling interval for checking new files (default: 3.0 seconds)
- `--workers <count>` - Number of parallel workers for media processing (default: CPU core count)
- `--bitrate <mbps>` - Target maximum bitrate for transcoding (default: 2.5 Mbps)
- `--ingest-username-from <method>` - How to determine username for files in incoming/ folder (default: `file-owner`)

#### Directory Structure

```
<data_dir>/
├── incoming/          # Drop files here for automatic processing
├── videos/           # Processed media storage (organized by media ID)
├── rejected/         # Files that failed processing
└── upload/           # Temporary storage for web uploads
```

#### How It Works

1. **File Detection**: The system continuously polls `<data_dir>/incoming/` for new files
2. **Write Completion**: Waits for file size to stabilize between polls to ensure upload completion
3. **Username Resolution**: Determines the Clapshot user based on the `--ingest-username-from` setting:
   - `file-owner` (default): Uses OS-level file ownership to determine the user (e.g., file owned by `alice` becomes owned by Clapshot user `alice`)
   - `folder-name`: Uses the first subdirectory name in the file path as the username (e.g., `incoming/alice/video.mp4` assigns to user `alice`)
4. **User Assignment**: Files are assigned to users based on the resolved username
5. **Processing**: Files are moved to permanent storage, transcoded if needed, and added to the user's media library

#### Important Notes

- **Username Mapping**:
  - In `file-owner` mode: The system directly maps OS file owners to Clapshot user IDs with no translation layer
  - In `folder-name` mode: Username is extracted from the first subdirectory path, enabling (S)FTP uploads without OS-level user accounts
  - Note that for web uploads, Clapshot server trusts the reverse proxy's HTTP headers for both username and display name, so any user mappings should happen at the proxy level
- **User Auto-Creation**: If a user doesn't exist in the database, they are automatically created when their first file is processed
- **Directory Processing**:
  - Both modes process files in `incoming/` and one level of subdirectories (e.g., `incoming/username/file.mp4`)
  - This allows for atomic moves from staging directories (e.g., `incoming/username/incomplete/` → `incoming/username/`) to avoid processing incomplete uploads
  - In `folder-name` mode: Username is extracted from the first directory level in the path
- **Error Handling**: Files that fail processing are moved to the `rejected/` directory with error details
- **Duplicate Prevention**: The system prevents re-processing of identical files using content-based hashing

#### Security Considerations

User assignment security depends on the chosen method:

**For `file-owner` mode:**
- File system permissions align with your desired user access model
- OS usernames match your intended Clapshot user identifiers
- The incoming directory has appropriate write permissions for authorized users
- Consider using group ownership and umask settings for shared environments

**For `folder-name` mode:**
- Ensure write permissions are properly restricted on the `incoming/` directory
- **File Permissions**: Files must be writable by the OS user running Clapshot Server (often `www-data`) so they can be moved during processing. Use group ownership with sticky bits or appropriate umask settings (e.g., `chmod g+s incoming/` and ensure uploaded files have group write permissions)
- Consider that any user who can create directories in `incoming/` can impersonate other users
- This mode is ideal for (S)FTP scenarios where you control directory creation through the FTP server configuration

### Notifications (e‑mail / Slack / …)

Clapshot can call an external script on comment, user-message, and media-file
events so you can send e‑mail, Slack messages, etc. It is **off by default**;
enable it with `notification-script` (and optionally `notification-events`) in
the config. See the **[Notification Hook guide](notification-hook.md)** for setup,
the event list, and the JSON payload reference.

### Docker Environment Configuration

Clapshot's Docker demo containers support comprehensive configuration via environment variables, allowing you to customize server behavior without rebuilding images or mounting custom config files.

#### Variable Naming Convention

Use the `CLAPSHOT_SERVER__` prefix with uppercase and underscores:

```bash
CLAPSHOT_SERVER__OPTION_NAME=value
```

The system automatically converts these to config file format:
- `CLAPSHOT_SERVER__INGEST_USERNAME_FROM` → `ingest-username-from`
- `CLAPSHOT_SERVER__DEBUG` → `debug`
- `CLAPSHOT_SERVER__URL_BASE` → `url-base`

#### Common Configuration Examples

**Basic Setup:**
```bash
# Single-user demo with custom URL
docker run --rm -it -p 8080:80 \
  -e CLAPSHOT_SERVER__URL_BASE=http://clapshot.example.com/ \
  -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo
```

**Folder-based Username Assignment:**
```bash
# Multi-user demo with folder-based usernames for SFTP support
docker run --rm -it -p 8080:80 \
  -e CLAPSHOT_SERVER__INGEST_USERNAME_FROM=folder-name \
  -e CLAPSHOT_SERVER__URL_BASE=http://clapshot.example.com/ \
  -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo-htwicket
```

**Development Configuration:**
```bash
# Enable debug logging and custom bitrate
docker run --rm -it -p 8080:80 \
  -e CLAPSHOT_SERVER__DEBUG=true \
  -e CLAPSHOT_SERVER__BITRATE=5.0 \
  -e CLAPSHOT_SERVER__WORKERS=4 \
  -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo
```

#### Available Configuration Options

All options from the server config file are supported. Most commonly used:

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `CLAPSHOT_SERVER__URL_BASE` | User-facing URL | `http://127.0.0.1:8080` |
| `CLAPSHOT_SERVER__DATA_DIR` | Database and media location | `/mnt/clapshot-data/data` |
| `CLAPSHOT_SERVER__INGEST_USERNAME_FROM` | Username assignment method | `file-owner` |
| `CLAPSHOT_SERVER__DEBUG` | Enable verbose logging | `false` |
| `CLAPSHOT_SERVER__BITRATE` | Transcoding bitrate (Mbps) | `2.5` |
| `CLAPSHOT_SERVER__WORKERS` | Transcoding workers | `0` (auto) |
| `CLAPSHOT_SERVER__POLL` | Incoming folder poll interval | `3` |
| `CLAPSHOT_SERVER__CORS` | CORS origins | Same as `url-base` |

For the complete list, see the [server configuration file](../server/debian/additional_files/clapshot-server.conf).

**Client Configuration:**

| Variable | Description | Default |
|----------|-------------|---------|
| `CLAPSHOT_APP_TITLE` | Application title | `Clapshot` |
| `CLAPSHOT_LOGO_URL` | Logo image URL | `clapshot-logo.svg` |

#### Docker Compose Example

> This snippet only **illustrates env-var configuration** on the single-container demo image — it
> is *not* a production deployment. For real Docker deployments, use the maintained
> [Compose recipes](../deploy/compose/) (the same `CLAPSHOT_SERVER__*` variables apply, set in `.env`).

```yaml
version: '3.8'
services:
  clapshot:
    image: elonen/clapshot:latest-demo-htwicket
    ports:
      - "8080:80"
    volumes:
      - clapshot-data:/mnt/clapshot-data/data
    environment:
      - CLAPSHOT_SERVER__URL_BASE=http://localhost:8080/
      - CLAPSHOT_SERVER__INGEST_USERNAME_FROM=folder-name
      - CLAPSHOT_SERVER__DEBUG=false
      - CLAPSHOT_SERVER__BITRATE=3.0
      - CLAPSHOT_APP_TITLE=Company Media Review

volumes:
  clapshot-data:
```
****