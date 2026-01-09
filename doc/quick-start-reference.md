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

# Multi-user demo with basic auth
docker run --rm -it -p 8080:80 -v clapshot-demo:/mnt/clapshot-data/data elonen/clapshot:latest-demo-htadmin
```
**Access:** `http://127.0.0.1:8080`

### LAN Access (Multiple Machines)
```bash
# Replace YOUR_IP with your machine's LAN IP (e.g., 192.168.1.100)
# Note: Also expose WebSocket port 8095 for live annotations
docker run --rm -it -p 8080:80 -p 8095:8095 \
  -e CLAPSHOT_SERVER__URL_BASE="http://YOUR_IP:8080/" \
  -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo-htadmin

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
  elonen/clapshot:latest-demo-htadmin
```
**Access:** `http://YOUR_IP:8025`

## Production Deployments

### Docker Compose (Recommended)
```yaml
version: '3.8'
services:
  clapshot:
    image: elonen/clapshot:latest-demo-htadmin
    container_name: clapshot_prod
    environment:
      - CLAPSHOT_SERVER__URL_BASE=https://clapshot.yourdomain.com/
      - CLAPSHOT_SERVER__CORS=https://clapshot.yourdomain.com
    ports:
      - "8080:80"
    volumes:
      - clapshot-data:/mnt/clapshot-data/data
    restart: unless-stopped

volumes:
  clapshot-data:
```

### Cloudflare Tunnel (Internet Access)
```bash
# Download and run the Cloudflare script
wget https://raw.githubusercontent.com/elonen/clapshot/master/test/run-cloudflare.sh
chmod +x run-cloudflare.sh
./run-cloudflare.sh
```

### Behind Reverse Proxy (nginx, Traefik, etc.)
```bash
# Clapshot runs on internal port, proxy handles HTTPS
docker run -d \
  -e CLAPSHOT_SERVER__URL_BASE="https://clapshot.company.com/" \
  -e CLAPSHOT_SERVER__CORS="https://clapshot.company.com" \
  -p 127.0.0.1:8080:80 -p 127.0.0.1:8095:8095 \
  -v clapshot-data:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo-htadmin
```

## Linux VM Installation

### Debian/Ubuntu Automated Setup
```bash
# Download the installation script
wget https://gist.githubusercontent.com/elonen/80a721f13bb4ec1378765270094ed5d5/raw/d333729c6a8df88edc3825b69bd571ba89879eee/install-clapshot-deb12.sh

# Run with your public address (required)
sudo bash install-clapshot-deb12.sh -a http://YOUR_IP:8080
# Or for HTTPS: sudo bash install-clapshot-deb12.sh -a https://clapshot.yourdomain.com
```

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

**Note:** Legacy variable names like `CLAPSHOT_URL_BASE`, `CLAPSHOT_CORS`, etc. still work for backward compatibility, but the `CLAPSHOT_SERVER__` format is recommended.

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

## Default Credentials (Change These!)

**Clapshot Users:**
- `admin:admin` (can edit all videos)
- `demo:demo`
- `alice:alice123`
- `bob:bob123` (cannot upload files)

**User Management:**
- `htadmin:admin` (access `/htadmin/` for user management)

> ⚠️ **Security Warning:** Change all default passwords before sharing with others!

## Need More Help?

- **Detailed troubleshooting:** [Connection Troubleshooting Guide](connection-troubleshooting.md)
- **Advanced configuration:** [Sysadmin Guide](sysadmin-guide.md)
- **Architecture details:** [README.md](../README.md)
- **Report issues:** [GitHub Issues](https://github.com/elonen/clapshot/issues)