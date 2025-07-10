# Clapshot Connection Troubleshooting Guide

> **Quick Start:** For common deployment scenarios without detailed explanation, see the [Quick Start Reference](quick-start-reference.md).

> **Architecture:** For detailed understanding of how Clapshot components communicate, see the [Architecture Overview](architecture-overview.md).

This guide addresses common connection issues between the Clapshot Client (browser), Server, and Nginx components.

## Common Connection Problems

### 1. "Connecting server" - Stuck Loading

**Symptoms:**

- Browser shows "Connecting server" message indefinitely
- Console shows `NetworkError: Failed to fetch` or `502 Bad Gateway`
- Cannot access `/api/health` endpoint

This typically indicates problems during [Phase 2: WebSocket Session Initialization](architecture-overview.md#phase-2-websocket-session-initialization) of the communication flow.

**Causes & Solutions:**

#### A. Client Configuration Issues

The client needs to know where to connect. This is controlled by the `clapshot_client.conf.json` configuration file.

**For manual deployments:**

```json
{
  "ws_url": "http://YOUR_HOST:YOUR_PORT/api/ws",
  "upload_url": "http://YOUR_HOST:YOUR_PORT/api/upload",
  ...
}
```

**For Docker deployments:**

```bash
# Set the URL base environment variable - this automatically configures the client
docker run ... -e CLAPSHOT_URL_BASE="http://YOUR_HOST:YOUR_PORT/" ...
```

*Note: `CLAPSHOT_URL_BASE` environment variable is just a Docker convenience feature. The Docker startup script uses it to automatically generate the proper `clapshot_client.conf.json` file.*


**Location of config file:**

- Docker: Automatically generated in `/etc/clapshot_client.conf`
- Debian package: `/etc/clapshot_client.conf` (symlink to `/usr/share/clapshot-client/www/clapshot_client.conf.json`)
- Must be accessible by the web browser alongside HTML/JS/CSS files.

#### B. Port Mapping Issues

**Problem:** Client tries to connect to hardcoded localhost:8080

```
Failed to fetch 'http://127.0.0.1:8080/api/health'
```

**Solution for Docker:**

Let's say you have Nginx listening to port 80 inside Docker, and have mapped it to YOUR_ADDRESS:8025 on the outside:

```bash
# Wrong - port mismatch
docker run -p 8025:80 elonen/clapshot:latest-demo

# Right - configure client to match
docker run -p 8025:80 -e CLAPSHOT_URL_BASE="http://YOUR_ADDRESS:8025/" elonen/clapshot:latest-demo
```

This will cause Client to connect `http://YOUR_ADDRESS:8025/api/health` instead.

**Solution for Debian deployment:**

Edit `/etc/clapshot_client.conf` and set `CLAPSHOT_URL_BASE` accordingly.

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

These issues prevent [Phase 3: Interaction with Organizer and Database](architecture-overview.md#phase-3-interaction-with-organizer-and-database) from functioning properly.

**Common Causes:**

#### A. Duplicate CORS Configuration
```
DuplicateOptionError: option 'cors' in section 'general' already exists
```

**Solution:** Remove duplicate CORS entries from `/etc/clapshot-server.conf`.

#### B. Missing Required Arguments
```
error: the following required arguments were not provided:
  --data-dir 
  --url-base 
```

**Solution:** Ensure proper configuration in service file or use Docker environment variables. 

**Examples and documentation:**

- See [Quick Start Reference](quick-start-reference.md) for working Docker command examples
- See [Sysadmin Guide](sysadmin-guide.md) for manual installation and configuration details
- Check existing Docker commands in this guide for required argument patterns

### 3. Cannot Access HTAdmin Interface (`/htadmin`)

(This only applies if you are using the example authentication method, HTAdmin + Basic Auth)

**Symptoms:**

- Cannot access `http://YOUR_HOST:YOUR_PORT/htadmin`
- 404 or connection refused errors on admin endpoints
- Admin interface works locally but not remotely

**Causes & Solutions:**

#### A. Docker Image Variants

Some Docker images include a simple HTTP basic auth admin interface at `/htadmin`:

- `elonen/clapshot:latest-demo-htadmin` - includes basic auth admin
- `elonen/clapshot:latest-demo` - no auth

#### B. Network Access Issues

The admin interface may be configured to only accept local connections:

```bash
# Test local access first
curl http://localhost:8080/htadmin

# If that works but remote doesn't, check network config
curl http://YOUR_IP:8080/htadmin
```

### 4. CORS and Cross-Origin Issues

**What is CORS?**

Cross-Origin Resource Sharing (CORS) is a crucial web security mechanism that controls which websites can access API of your Clapshot server. When the browser's client code runs on one domain (like `http://192.168.1.100:8080`) but tries to connect to API endpoints on another domain or port, the browser enforces CORS policies to prevent malicious website's Javascript components from accessing APIs they have not business accessing. Proper CORS configuration is essential for security while allowing legitimate access to your Clapshot instance.

**Symptoms:**
- CORS errors in browser's developer console
- API calls blocked

CORS issues typically affect [Phase 4: Thumbnail Retrieval](architecture-overview.md#phase-4-thumbnail-retrieval) and [Phase 5: Video Playback](architecture-overview.md#phase-5-video-playback).

**Solutions:**

- **For production:**
```
cors = 'https://yourdomain.com'
```

- **For development:**
```
cors = '*'
```

CAUTION: Using '*' in production could expose your users' data to malicious actors! Use it _only_ for local develpment.

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
cat /etc/clapshot_client.conf
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

For more details on how these components interact, see the [Detailed Communication Flow](architecture-overview.md#detailed-communication-flow).

## Docker vs Debian Package Deployment

### When to Use Docker

- **Development/testing**: Quick setup with minimal configuration
- **Self-hosted with reverse proxy**: Docker behind nginx-proxy-manager, Traefik, etc.
- **Simple deployments**: Single-machine setup with all components in one container

### When to Use Debian Packages

- **Production deployments**: More control over individual components
- **Custom authentication**: Integration with existing auth systems
- **Performance optimization**: Fine-tuned nginx and server configurations
- **System integration**: Proper systemd services and logging

### Understanding the Architecture

Both deployment methods include the same core components:

- **Client**: Web interface (HTML/JS/CSS files)
- **Server**: Rust backend API
- **Database**: SQLite for metadata
- **Organizer**: Python plugin for file management
- **Nginx**: Web server and reverse proxy

The main difference is how these components are packaged and configured. See [Architecture Overview](architecture-overview.md) for detailed component interactions.

## Docker-Specific Troubleshooting

### Working Docker Compose Example

```yml
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

### Docker-to-Docker Communication (Reverse Proxy)

When running Clapshot behind another Docker container (like nginx-proxy-manager):

```yaml
version: '3.8'

services:
  clapshot:
    image: elonen/clapshot:latest-demo-htadmin
    container_name: clapshot_demo
    environment:
      # Use the external domain users will access
      - CLAPSHOT_URL_BASE=https://clapshot.yourdomain.com/
      - CLAPSHOT_CORS=https://clapshot.yourdomain.com
    # Don't expose ports directly - let reverse proxy handle it
    # ports:
    #   - "8080:80"
    volumes:
      - clapshot-data:/mnt/clapshot-data/data
    networks:
      - proxy-network
    restart: unless-stopped

  nginx-proxy:
    # Your reverse proxy configuration
    networks:
      - proxy-network

networks:
  proxy-network:
    external: true

volumes:
  clapshot-data:
```

**Key points:**

- Set `CLAPSHOT_URL_BASE` to the external domain, not the container name
- Use Docker networks to allow containers to communicate
- Don't expose Clapshot ports directly if using a reverse proxy

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
| `database is locked` | Concurrent access | Stop all processes, restart cleanly |
| `Connecting server...` | Client/server mismatch | Verify URL configuration |
| CORS errors | Cross-origin policy | Configure CORS properly |

## Getting Help

When asking for help, please provide:

1. **Deployment method**: Docker, Debian package, manual build
2. **Client configuration**: Contents of `clapshot_client.conf.json`
3. **Server logs**: Last 50 lines of clapshot server logs
4. **Network setup**: How are you accessing Clapshot (localhost, LAN, internet)
5. **Error messages**: Complete error messages from browser's developer console

## Related Documentation

- [Architecture Overview](architecture-overview.md) - Understanding how components communicate
- [Quick Start Reference](quick-start-reference.md) - Common deployment scenarios
- [Sysadmin Guide](sysadmin-guide.md) - Advanced configuration
- [README.md](../README.md) - Basic setup instructions
- [Cloudflare example](../test/run-cloudflare.sh) - Docker deployment using Cloudflare as proxy
