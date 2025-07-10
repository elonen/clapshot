# Clapshot Connection Troubleshooting Guide

This guide addresses common connection issues between the Clapshot client (browser), server, and nginx components. These issues stem from the distributed architecture where the browser client needs to connect to both the nginx reverse proxy and the backend Clapshot server.

## Understanding Clapshot's Architecture

Before troubleshooting, it's important to understand how Clapshot components communicate:

```
Browser (Client) → Nginx → Clapshot Server + Organizer
     ↓                ↓            ↓
   SPA loads     Reverse proxy   API + WebSocket
 client config   Static files    (port 8095)
```

### Key Components:

1. **Client (Browser)**: Svelte SPA that loads `clapshot_client.conf.json` for server connection info
2. **Nginx**: Reverse proxy serving static files and proxying API calls to backend
3. **Clapshot Server**: Rust binary listening on port 8095 (API + WebSocket)
4. **Organizer**: Python process communicating with server via gRPC

## Common Connection Problems

### 1. "Connecting server" - Stuck Loading

**Symptoms:**
- Browser shows "Connecting server" message indefinitely
- Console shows `NetworkError: Failed to fetch` or `502 Bad Gateway`
- Cannot access `/api/health` endpoint

**Causes & Solutions:**

#### A. Client Configuration Issues

The client needs to know where to connect. Check `clapshot_client.conf.json`:

**For Docker deployments:**
```bash
# Set the URL base environment variable
docker run ... -e CLAPSHOT_URL_BASE="http://YOUR_HOST:YOUR_PORT/" ...
```

**For manual deployments:**
```json
{
  "ws_url": "http://YOUR_HOST:YOUR_PORT/api/ws",
  "upload_url": "http://YOUR_HOST:YOUR_PORT/api/upload",
  ...
}
```

**Location of config file:**
- Docker: Automatically generated in `/etc/clapshot_client.conf`
- Debian package: `/usr/share/clapshot-client/www/clapshot_client.conf.json`
- Must be accessible by the web browser alongside HTML/JS/CSS files

#### B. Port Mapping Issues

**Problem:** Client tries to connect to hardcoded localhost:8080
```
Failed to fetch 'http://127.0.0.1:8080/api/health'
```

**Solution for Docker:**
```bash
# Wrong - port mismatch
docker run -p 8025:80 elonen/clapshot:latest-demo

# Right - configure client to match
docker run -p 8025:80 -e CLAPSHOT_URL_BASE="http://YOUR_IP:8025/" elonen/clapshot:latest-demo
```

#### C. Network Access Issues

**Problem:** Client configured for localhost but accessing from different machine

**Solutions:**
- For local network access: Use actual IP address in `CLAPSHOT_URL_BASE`
- For internet access: Use proper domain name
- For development: Use `0.0.0.0:8080` for binding

### 2. Server Startup Failures

**Symptoms:**
- Server logs show errors and exits
- 502 Bad Gateway errors
- Missing gRPC server section in logs

**Common Causes:**

#### A. Duplicate CORS Configuration
```
DuplicateOptionError: option 'cors' in section 'general' already exists
```

**Solution:** Remove duplicate CORS entries from `/etc/clapshot-server.conf` or let Docker script handle it automatically.

#### B. Missing Required Arguments
```
error: the following required arguments were not provided:
  --data-dir 
  --url-base 
```

**Solution:** Ensure proper configuration in service file or use Docker environment variables.

#### C. Database Lock Issues
```
Failed to get migrations: DatabaseError(Unknown, "database is locked")
```

**Solution:** 
- Stop all Clapshot processes
- Check for stale lock files
- Restart with clean database state

### 3. CORS and Cross-Origin Issues

**Symptoms:**
- CORS errors in browser console
- API calls blocked

**Solutions:**

**For development:**
```
cors = '*'
```

**For production:**
```
cors = 'https://yourdomain.com'
```

**For Docker:**
```bash
docker run ... -e CLAPSHOT_CORS="https://yourdomain.com" ...
```

## Step-by-Step Troubleshooting

### Step 1: Check Server Status

```bash
# Check if server is running
curl http://localhost:8095/api/health

# Check server logs
tail -f /var/log/clapshot.log
# or for Docker:
docker logs container_name
```

### Step 2: Verify Client Configuration

```bash
# Check client config
cat /usr/share/clapshot-client/www/clapshot_client.conf.json
# or for Docker:
docker exec container_name cat /etc/clapshot_client.conf
```

Ensure URLs point to correct host and port.

### Step 3: Test Network Connectivity

```bash
# From client machine, test API access
curl http://YOUR_HOST:YOUR_PORT/api/health

# Test WebSocket connectivity
wscat -c ws://YOUR_HOST:YOUR_PORT/api/ws
```

### Step 4: Check Nginx Configuration

```bash
# Verify nginx is proxying correctly
nginx -t
systemctl status nginx

# Check nginx access/error logs
tail -f /var/log/nginx/access.log
tail -f /var/log/nginx/error.log
```

### Step 5: Verify Component Communication

```bash
# Check if organizer is connected
grep "org->srv connected" /var/log/clapshot.log

# Verify gRPC communication
ls -la /mnt/clapshot-data/data/grpc-*.sock
```

## Docker-Specific Troubleshooting

### Working Docker Compose Example

```yaml
version: '3.8'

services:
  clapshot:
    image: elonen/clapshot:latest-demo-htadmin
    container_name: clapshot_demo
    environment:
      - CLAPSHOT_URL_BASE=http://YOUR_IP:8080/
      # Optional: Set custom CORS
      # - CLAPSHOT_CORS=http://YOUR_IP:8080
    ports:
      - "8080:80"
    volumes:
      - clapshot-data:/mnt/clapshot-data/data
    restart: unless-stopped

volumes:
  clapshot-data:
```

### Environment Variables

- `CLAPSHOT_URL_BASE`: Full URL where users will access Clapshot
- `CLAPSHOT_CORS`: CORS origins (defaults to URL_BASE)
- `CLAPSHOT_APP_TITLE`: Custom application title
- `CLAPSHOT_LOGO_URL`: Custom logo URL

### Container Network Issues

```bash
# Check container networking
docker network ls
docker inspect container_name | grep -A 10 NetworkSettings

# Test internal connectivity
docker exec -it container_name curl http://localhost:8095/api/health
```

## Production Deployment Considerations

### 1. Use Proper Domains, Not IP Addresses

```bash
# Good
CLAPSHOT_URL_BASE="https://clapshot.company.com/"

# Avoid in production
CLAPSHOT_URL_BASE="http://192.168.1.100:8080/"
```

### 2. Enable HTTPS

Use reverse proxy (nginx, Cloudflare, etc.) to provide HTTPS:

```bash
# With Cloudflare tunnels
docker run ... -e CLAPSHOT_URL_BASE="https://your-tunnel.trycloudflare.com/" ...
```

### 3. Secure CORS Configuration

```bash
# Don't use wildcards in production
CLAPSHOT_CORS="https://clapshot.company.com"
```

### 4. Authentication Setup

Ensure authentication headers are properly forwarded:
- `X-Remote-User-Id`
- `X-Remote-User-Name` 
- `X-Remote-User-Is-Admin`

## Quick Reference: Common Error Messages

| Error Message | Likely Cause | Solution |
|---------------|--------------|----------|
| `NetworkError: Failed to fetch` | Client can't reach server | Check URL configuration |
| `502 Bad Gateway` | Server not running | Check server startup logs |
| `DuplicateOptionError: cors` | Config file corruption | Remove duplicate CORS entries |
| `database is locked` | Concurrent access | Stop all processes, restart cleanly |
| `Connecting server...` | Client/server mismatch | Verify URL configuration |
| CORS errors | Cross-origin policy | Configure CORS properly |

## Getting Help

When asking for help, please provide:

1. **Deployment method**: Docker, Debian package, manual build
2. **Client configuration**: Contents of `clapshot_client.conf.json`
3. **Server logs**: Last 50 lines of clapshot server logs
4. **Network setup**: How are you accessing Clapshot (localhost, LAN, internet)
5. **Error messages**: Complete error messages from browser console

## Related Documentation

- [Sysadmin Guide](sysadmin-guide.md) - Advanced configuration
- [README.md](../README.md) - Basic setup instructions
- [Cloudflare example](../test/run-cloudflare.sh) - Production Docker deployment