# Clapshot Quick Start Reference

Quick reference for common Clapshot deployment scenarios. For detailed troubleshooting, see the [Connection Troubleshooting Guide](connection-troubleshooting.md).

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
docker run --rm -it -p 8080:80 \
  -e CLAPSHOT_URL_BASE="http://YOUR_IP:8080/" \
  -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo-htadmin
```
**Access:** `http://YOUR_IP:8080`

### Custom Port
```bash
# Using port 8025 instead of 8080
docker run --rm -it -p 8025:80 \
  -e CLAPSHOT_URL_BASE="http://YOUR_IP:8025/" \
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
      - CLAPSHOT_URL_BASE=https://clapshot.yourdomain.com/
      - CLAPSHOT_CORS=https://clapshot.yourdomain.com
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
  -e CLAPSHOT_URL_BASE="https://clapshot.company.com/" \
  -e CLAPSHOT_CORS="https://clapshot.company.com" \
  -p 127.0.0.1:8080:80 \
  -v clapshot-data:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo-htadmin
```

## Linux VM Installation

### Debian/Ubuntu Automated Setup
```bash
# Download and run the installation script
wget https://gist.githubusercontent.com/elonen/80a721f13bb4ec1378765270094ed5d5/raw/install-clapshot.sh
sudo bash install-clapshot.sh
```

## Common Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `CLAPSHOT_URL_BASE` | Full URL where users access Clapshot | `https://clapshot.company.com/` |
| `CLAPSHOT_CORS` | CORS allowed origins | `https://clapshot.company.com` |
| `CLAPSHOT_APP_TITLE` | Custom application title | `"Video Review System"` |
| `CLAPSHOT_LOGO_URL` | Custom logo URL | `"/custom-logo.svg"` |

## Quick Diagnostics

### Check if Server is Running
```bash
# Test API health endpoint
curl http://localhost:8080/api/health

# Check Docker container logs
docker logs container_name
```

### Verify Client Configuration
```bash
# Check client config (adjust path as needed)
curl http://localhost:8080/clapshot_client.conf.json
```

### Test Network Connectivity
```bash
# From another machine, test access
curl http://YOUR_IP:8080/api/health
```

## Common Error Solutions

| Error | Quick Fix |
|-------|-----------|
| "Connecting server..." | Set `CLAPSHOT_URL_BASE` environment variable |
| 502 Bad Gateway | Check server logs, likely server startup failure |
| NetworkError: Failed to fetch | Check client config and network connectivity |
| CORS errors | Set `CLAPSHOT_CORS` to match your domain |

## Default Credentials (Change These!)

**Clapshot Users:**
- `admin:admin` (can edit all videos)
- `demo:demo`
- `alice:alice123`

**User Management:**
- `htadmin:admin` (access `/htadmin/` for user management)

> ⚠️ **Security Warning:** Change all default passwords before sharing with others!

## Need More Help?

- **Detailed troubleshooting:** [Connection Troubleshooting Guide](connection-troubleshooting.md)
- **Advanced configuration:** [Sysadmin Guide](sysadmin-guide.md)
- **Architecture details:** [README.md](../README.md)
- **Report issues:** [GitHub Issues](https://github.com/elonen/clapshot/issues)