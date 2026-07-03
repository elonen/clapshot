# Clapshot Quick Start Reference

Quick reference for common Clapshot deployment scenarios. For detailed troubleshooting, see the [Connection Troubleshooting Guide](connection-troubleshooting.md).

> **⚠️ Important: Set URL Base First!** 
> 
> Before you start, configure the base URL so Clapshot knows where clients will connect. Without this, browsers on other machines will keep retrying `127.0.0.1` and never reach your server!
>
> - **Docker:** Use `-e CLAPSHOT_SERVER__URL_BASE="http://YOUR_IP:8080/"`
> - **Native install:** Edit `/etc/clapshot-server.conf` and set `url-base` and `cors` under `[general]`

> **Architecture:** For detailed understanding of how Clapshot components communicate, see the [Architecture Overview](architecture-overview.md).

## Local Development/Testing

### Single Machine Demo (Localhost Only)
```bash
# Basic demo - no authentication
docker run --rm -it -p 8080:80 -v clapshot-demo:/mnt/clapshot-data/data elonen/clapshot:latest-demo

# Multi-user demo with login (HTWicket)
docker run --rm -it -p 8080:80 -v clapshot-demo:/mnt/clapshot-data/data elonen/clapshot:latest-demo-htwicket
```
**Access:** `http://127.0.0.1:8080`

### LAN Access (Multiple Machines)
```bash
# Replace YOUR_IP with your machine's LAN IP (e.g., 192.168.1.100)
# Note: Also expose WebSocket port 8095 for live annotations
docker run --rm -it -p 8080:80 -p 8095:8095 \
  -e CLAPSHOT_SERVER__URL_BASE="http://YOUR_IP:8080/" \
  -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo-htwicket

# If behind a firewall, allow both ports:
# ufw allow 8080/tcp
# ufw allow 8095/tcp
```
**Access:** `http://YOUR_IP:8080`

### Custom Port
```bash
# Using port 8025 instead of 8080
docker run --rm -it -p 8025:80 \
  -e CLAPSHOT_SERVER__URL_BASE="http://YOUR_IP:8025/" \
  -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo-htwicket
```
**Access:** `http://YOUR_IP:8025`

### Cloudflare Tunnel

To expose a *demo* over a temporary public URL, [`test/run-cloudflare.sh`](../test/run-cloudflare.sh)
runs the single-container demo image behind a Cloudflare tunnel (free plan limits upload size/time).
For real internet use, deploy a Compose recipe with `CADDY_CERT_DOMAIN` set instead.

```bash
# Download and run the Cloudflare demo script
wget https://raw.githubusercontent.com/elonen/clapshot/master/test/run-cloudflare.sh
chmod +x run-cloudflare.sh
./run-cloudflare.sh
```

## Production Deployments

> **Don't use the single-image `latest-demo*` containers for production** — they're for local
> evaluation only. Real deployments use the **[Docker Compose recipes](../deploy/compose/)** (Docker)
> or the **[`.deb` packages](../deploy/debian/)** (VM/bare-metal). The snippets below show the
> URL/auth settings you'd put in those recipes' `.env` files.

### Docker Compose

Deploy a ready-made recipe from [`deploy/compose/`](../deploy/compose/) — `htwicket` (simple
password login) or `custom-proxy` (your own IdP),. Caddy fetches Let's Encrypt certificates
automatically; the only file you edit is `.env`:

```ini
# deploy/compose/htwicket/.env
CLAPSHOT_URL_BASE=https://clapshot.yourdomain.com/
CADDY_CERT_DOMAIN=clapshot.yourdomain.com     # automatic Let's Encrypt
```

```sh
cd deploy/compose/htwicket
cp .env.example .env        # then edit it
docker compose up -d
```

See [deploy/compose/README.md](../deploy/compose/README.md) for Portainer/Komodo, HTTPS modes, and upgrades.

### Behind your own reverse proxy / IdP

Already running nginx, Traefik, Authentik, oauth2-proxy, Kerberos, etc.? Use the
[`custom-proxy`](../deploy/compose/custom-proxy/) recipe — it trusts the `X-Remote-User-*`
headers your proxy sets, and your proxy terminates TLS. See
[Advanced Authentication](sysadmin-guide.md#advanced-authentication).


## Linux VM Installation

### Debian/Ubuntu Automated Setup

Install the `.deb` packages on a Debian 12/13 host with the
[install script](../deploy/debian/install-clapshot-deb.sh). Read it first, then run as root:

```bash
# Download the installation script (read it before running)
wget https://raw.githubusercontent.com/elonen/clapshot/master/deploy/debian/install-clapshot-deb.sh

sudo bash install-clapshot-deb.sh
```

Then set your public URL in `/etc/clapshot-server.conf` (see below). For details and
authentication/HTTPS setup, see [deploy/debian/README.md](../deploy/debian/README.md).

**Manual configuration (if needed later):**

```ini
# Edit /etc/clapshot-server.conf
[general]
url-base = http://YOUR_IP:8080/
cors     = http://YOUR_IP:8080

# Then restart the service
sudo systemctl restart clapshot-server
```

## Common Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `CLAPSHOT_SERVER__URL_BASE` | Full URL where users access Clapshot | `https://clapshot.company.com/` |
| `CLAPSHOT_SERVER__CORS` | CORS allowed origins | `https://clapshot.company.com` |
| `CLAPSHOT_SERVER__DEBUG` | Enable verbose server logging | `true` |
| `CLAPSHOT_SERVER__INGEST_USERNAME_FROM` | Username assignment method | `folder-name` |
| `CLAPSHOT_APP_TITLE` | Custom application title | `"Video Review System"` |
| `CLAPSHOT_LOGO_URL` | Custom logo URL | `"/custom-logo.svg"` |

**Note:** Legacy variable names like `CLAPSHOT_URL_BASE`, `CLAPSHOT_CORS`, etc. still work for backward compatibility, but the `CLAPSHOT_SERVER__` format is recommended as it works for all parameters.

**For comprehensive Docker configuration options using environment variables, see the [Docker Environment Configuration](sysadmin-guide.md#docker-environment-configuration) section in the Sysadmin Guide.**

## Quick Diagnostics

### Check if Server is Running
```bash
# Test API health endpoint
curl http://localhost:8080/api/health

# Check Docker container logs
docker logs container_name

# Check native installation logs
tail -f /var/log/clapshot.log
```

### Verify Client Configuration
```bash
# Check client config (adjust path as needed)
curl http://localhost:8080/clapshot_client.conf.json
```

### Client Configuration Options

The client configuration file (`clapshot_client.conf.json`) supports several optional settings:

| Option | Default | Description |
|--------|---------|-------------|
| `enable_mediabunny` | `true` | Enable frame-accurate video decoder using WebCodecs. If `false`, falls back to HTML5 video element for all seeking operations. |

**Note on `enable_mediabunny`:** The WebCodecs-based decoder (Mediabunny) provides precise frame-by-frame navigation but currently has a known limitation with color space handling - decoded frames may appear slightly darker than HTML5 video playback (see [Chromium issue #40061457](https://issues.chromium.org/issues/40061457)). If accurate color reproduction is more critical than frame-accurate stepping for your workflow, set `"enable_mediabunny": false` in the client config.

### Test Network Connectivity
```bash
# From another machine, test access
curl http://YOUR_IP:8080/api/health
```

## Common Error Solutions

| Error | Quick Fix |
|-------|-----------|
| "Connecting server..." | Set `CLAPSHOT_SERVER__URL_BASE` environment variable |
| 502 Bad Gateway | Check server logs, likely server startup failure |
| NetworkError: Failed to fetch | Check client config and network connectivity |
| CORS errors | Set `CLAPSHOT_SERVER__CORS` to match your domain |

**Browser troubleshooting:** Open DevTools Console (F12) to check for CORS/WebSocket errors like `ERR_CONNECTION_REFUSED` or 403 responses. These usually indicate network or configuration issues. See [Connection Troubleshooting Guide](connection-troubleshooting.md) for detailed help.

## Default Credentials (only in demo image)

**Clapshot Users:**

- `admin` / *random password printed in the container log* (can edit all videos; override with `CLAPSHOT_ADMIN_PASSWORD`)
- `demo:demo`
- `alice:alice123`
- `bob:bob123` (cannot upload files)

**User Management:**
- Log in as `admin` and open `/htwicket/admin` to manage users

## Need More Help?

- **Detailed troubleshooting:** [Connection Troubleshooting Guide](connection-troubleshooting.md)
- **Advanced configuration:** [Sysadmin Guide](sysadmin-guide.md)
- **Architecture details:** [README.md](../README.md)
- **Report issues:** [GitHub Issues](https://github.com/elonen/clapshot/issues)