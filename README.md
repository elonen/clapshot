# Clapshot: Self-Hosted Video/Media Review Tool
[![Release](https://img.shields.io/github/v/release/elonen/clapshot?include_prereleases)]() [![Build and test](https://github.com/elonen/clapshot/actions/workflows/docker-test.yml/badge.svg)](https://github.com/elonen/clapshot/actions/workflows/docker-test.yml)

## Overview

Clapshot is an open-source, self-hosted tool for collaborative video/media review and annotation. It features a Rust-based API server and a Svelte-based web UI. This tool is ideal for scenarios requiring local hosting of videos due to:

1. Policy constraints (*enterprise users*), or
2. Cost-benefit concerns against paid cloud services (*very small businesses*)

![Review UI screenshot](doc/video-commenting.webp)

### Key Features

- **Media Support**: Video, audio and image files with subtitle track management
- **Media Ingestion**: HTTP uploads with progress tracking, or monitored folder processing (files assigned by OS ownership)
- **Video Player**: Loop region control (i/o shortcuts), frame-by-frame navigation, comprehensive keyboard shortcuts
- **Collaborative Review**: Real-time synchronized playback, drawing annotations with 7-color palette, threaded comments
- **Professional Tools**: EDL import as time-coded comments, drawing undo/redo, timeline comment pins
- **File Organization**: Hierarchical folder system with drag-and-drop, admin user management interface
- **Media Processing**: FFmpeg transcoding with configurable quality, thumbnail generation
- **Authentication**: Reverse proxy integration supporting OAuth, JWT, Kerberos, SAML, etc.
- **Storage**: SQLite database with automatic migrations, file-based media storage
- **Extensibility**: Plugin system for custom workflows and integrations

*For a comprehensive feature list, see [FEATURES.md](FEATURES.md).*

![Video listing screenshot](doc/video-list.webp)

### When not to use Clapshot

If you don't require local hosting, or are not adept in networking and Linux, consider commercial cloud services which may offer more user-friendly interfaces and additional features out of the box.

## Demo

**Quick Start with Docker:**

- Local **single-user demo:** No authentication

```bash
docker run --rm -it -p 0.0.0.0:8080:80 -v clapshot-demo:/mnt/clapshot-data/data elonen/clapshot:latest-demo
```

- Local **multi-user demo** with HTTP basic auth:

```bash
docker run --rm -it -p 0.0.0.0:8080:80 -v clapshot-demo-htadmin:/mnt/clapshot-data/data elonen/clapshot:latest-demo-htadmin
```

After the Docker image starts, access the web UI at `http://127.0.0.1:8080`.

**Testing the demo:** Upload video/audio/image files via the web interface, or drop files into the container's `/mnt/clapshot-data/data/incoming/` directory for automatic processing. Try the keyboard shortcuts: spacebar (play/pause), 'i'/'o' (set loop points), 'l' (toggle loop), arrow keys (frame stepping).

The multi-user demo uses [PHP htadmin](https://github.com/soster/htadmin) for user management. Default credentials are shown in the terminal.

> **Note:** Chrome/Chromium works best. If accessing from a different machine, configure the `CLAPSHOT_SERVER__URL_BASE` environment variable (or legacy `CLAPSHOT_URL_BASE`). See the [Quick Start Reference](doc/quick-start-reference.md) for common deployment scenarios.


### Known Limitations

**Browser Compatibility:**
- **Desktop-first design:** Chrome/Chromium recommended for best compatibility
- **Mobile browsers:** Limited support - double-tap to open doesn't work on iOS/iPad, video player controls may not function properly, drawing annotations fail. Use desktop browsers for full functionality.

**Reverse Proxy Considerations:**
- **IIS:** Hard 2GB file upload limit (use monitored folder ingestion for larger files)
- **Cloudflare free tier:** ~100 second upload timeout can interrupt large file uploads

For large file uploads with these proxies, use the [monitored folder ingestion](doc/sysadmin-guide.md#monitored-folder-ingestion) feature with SFTP/SMB instead of HTTP uploads.

**Authentication:**
- **PHP htadmin:** The included htadmin example is simplistic and limited for user management and security. For production deployments, consider integrating with a modern identity provider (OAuth, LDAP, Kerberos, SAML, etc.) via reverse proxy. See [Advanced Authentication](doc/sysadmin-guide.md#advanced-authentication).


## Simple Small-business Production Deployments

Here are two alternative ways to deploy Clapshot + PHP Htadmin into a light production use:

### 1. Local Linux VM

If you have a virtualization platform (e.g. Proxmox) or a spare computer, here's
how to install and configure a Debian 12 host for Clapshot:

1. Prepare a Debian 12 with a mounted block device (or just directory) at `/mnt/clapshot-data`.
2. Download [Clapshot Debian Bookworm Deployment Script](https://gist.github.com/elonen/80a721f13bb4ec1378765270094ed5d5)
3. Run the script as root to install and auto-configure Clapshot.
4. **!! Change the default `admin` and `htadmin` passwords, and delete example users in Htadmin !!**

If you want to expose this to the Internet, you'll probably want to get HTTPS certificates with Let's Encrypt and use some reverse proxy to encrypt Clapshot traffic.

> **Security Note:** Monitored folder ingestion assigns files to users based on OS file ownership. Ensure file system permissions align with your intended user access model before enabling this feature.

### 2. Docker + Cloudflare (make public on the Web)

In this option, you'll run Clapshot + Htadmin in a Docker container (binding a local directory for Clapshot data),
and then start Cloudflared in another container to expose Clapshot to the Internet over an HTTPS tunnel.

> WARNING: Cloudflare – at least in the free plan – apparently limits HTTP upload times and/or sizes, so double check their offerings if you are planning to use this option for a production deployment.

1. Download and read [test/run-cloudflare.sh](test/run-cloudflare.sh), then run it
2. Once satisfied about operation, get a static domain on Cloudflare and modify the above script accordingly - or perhaps make a custom Docker Compose file
3. **!! Change the default `admin` and `htadmin` passwords, and delete example users in Htadmin !!**

The same process can be adapted to any other *HTTPS-Proxy-as-a-Service* besides Cloudflare. You'll probably need to pay them something.

## Configuration and Operation

**New to Clapshot?** Start with the [Quick Start Reference](doc/quick-start-reference.md) for common deployment scenarios.

**Need help?** Point your favorite LLM to [llms.txt](https://raw.githubusercontent.com/elonen/clapshot/refs/heads/master/llms.txt) for comprehensive configuration assistance. Most modern AI assistants (Claude, ChatGPT, Gemini, etc.) can read this file directly from the GitHub repository and help with installation, troubleshooting, and customization. Alternatively, you can try the [Clapshot Config Helper GPT](https://chatgpt.com/g/g-687debd7cfec8191ad14f604552f0121-clapshot-config-helper) (though it may have somewhat outdated documentation).

See the [Sysadmin Guide](doc/sysadmin-guide.md) for information on:

- configuring Nginx reverse proxy (for HTTPS and auth)
- using *systemd* for process management
- performing database migrations
- implementing advanced authentication methods
- building manually and running unit tests


**Having connection issues?** See the [Connection Troubleshooting Guide](doc/connection-troubleshooting.md) for help with common deployment and connectivity problems.

See [Upgrading Guide](doc/upgrading.md) for instructions on installing a new release over an old one.

**Want to customize media processing?** See the [Transcoding and Thumbnailing Guide](doc/transcoding.md) for configuring hardware acceleration, custom encoders, and specialized processing workflows.


## Architecture Overview

Main components:

- **Clapshot Client** – Single Page Application (SPA) that runs in the browser. Connects to Clapshot Server via Websocket. Written in *Svelte*.
- **Clapshot Server** – Linux daemon that handles most server-side logic. Binary written in *Rust*. Listens on `localhost` to the reverse proxy for plaintext HTTP and WSS.
- **Clapshot Organizer(s)** – Plugin(s) that organize media files into a custom folder hierarchy, etc. Written in Python (or any other language). See below for details.

Production deployments also depend on:

- **Web Browser** – Chrome/Chromium recommended for best compatibility. Loads and shows the Client.
- **Nginx Web Server** – SSL reverse proxy between Client and Server + static asset delivery for browser. Also routes session auth to Authentication Proxy.
- **Authentication Proxy** – Any auxiliary HTTP daemon that authenticates users and returns a **user id** and **username** in HTTP headers. In the demo, this is `/var/www/.htpasswd` + [PHP htadmin](https://github.com/soster/htadmin), but you can also use combinations like [Okta](https://www.okta.com/) + [Vouch](https://github.com/vouch/vouch-proxy) + [LDAP Authz Proxy](https://github.com/elonen/ldap_authz_proxy) or something equally advanced.

- **Sqlite DB** – Stores metadata, comments, user messages etc. Both Clapshot Server and Organizer(s) access this. This is just a file, not a daemon.
- **ffmpeg** and **mediainfo** – Clapshot Server processes media files with these commands.
- **File System** – Media files, HTML, JavaScript, CSS, thumbnail images etc, also `clapshot.sqlite`.

See [sequence diagram](doc/generated/open-frontpage-process.svg) for details on how these interact when a user opens the main page.

## Organizer Plugin System

Clapshot includes an extensible [Organizer Plugin system](doc/organizer-plugins.md) that enables custom workflows and integrations. Organizers use gRPC communication and can be implemented in any language.

The included "[basic_folders](organizer/basic_folders/README.md)" organizer (Python) provides:
- **Hierarchical Folders**: Personal folder structures for organizing media files
- **Folder Sharing**: Token-based sharing of folder contents (still requires authentication to access)
- **Admin Interface**: User management with batch operations and ownership transfer
- **Metaplugin extensions**: Easier extension in Python:

### Customization with Basic_Folders Metaplugins

**NEW: Add custom functionality by dropping a single Python file into `/opt/clapshot-org-bf-metaplugins`** – no need to modify core code or deal with gRPC protocol directly. Example use cases:

- **Add custom popup menu actions** to folders and media files (e.g., "Auto-subtitle", "Export to archive", "Send to review")
- **Implement custom workflows** and business logic specific to your organization (e.g. video rename, ownership transfer, auto-folders)
- **Integrate with external systems** (databases, LDAP, version controls, APIs) for authorization or processing
- **Modify the UI dynamically** based on user roles, folder properties, or file metadata
- **Run background process** such as automatic video expiration and trashing

This approach is **easier to develop** and **more robust against upgrades** than modifying core code or writing a full custom Organizer (if you're fine with Python). See [METAPLUGINS.md](organizer/basic_folders/METAPLUGINS.md) for complete documentation and a [working example](organizer/basic_folders/example_metaplugins/calculate_sha256.py).

## Development Setup

The [development setup guide](doc/development-setup.md) covers setting up the server and client development environments, and running local builds and tests.

## Contributions

Contributions are welcome, especially for features and improvements that benefit the wider user base. Please add your copyright notice for significant contributions.

### Contributors

  - **Client i18n, Chinese (Simplified) translations** – Mike-Solar

## Licensing

Copyright 2022 – 2025 by Jarno Elonen and contributors

- Clapshot Server and Client are licensed under the **GNU General Public License, GPLv2**.
- gRPC/proto3 libraries and example organizer plugins are under the **MIT License**.

This split licensing allows you to implement proprietary UIs and workflows through custom Organizer plugins without releasing them to the public.
