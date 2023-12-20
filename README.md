# Clapshot: Self-Hosted Video Review Tool
[![Release](https://img.shields.io/github/v/release/elonen/clapshot?include_prereleases)]() [![Build and test](https://github.com/elonen/clapshot/actions/workflows/docker-test.yml/badge.svg)](https://github.com/elonen/clapshot/actions/workflows/docker-test.yml)

## Overview

Clapshot is an open-source, self-hosted tool for collaborative video review and annotation. It features a Rust-based API server and a Svelte-based web UI. This tool is ideal for scenarios requiring local hosting of videos due to:
1. policy constraints (*enterprise users*), or
2. cost-benefit concerns against paid cloud services (*very small businesses*)

![Review UI screenshot](doc/video-commenting.webp)

**Key Features:**
- Video ingestions by HTTP video uploads, or shared folders
- Video transcoding with FFMPEG
- Commenting, drawing annotations, and threaded replies
- Real-time collaborative review sessions
- Storage of videos as files, and metadata in an SQLite (3.5+) database
- Authentication agnostic, you can use *OAuth, JWS, Kerberos, Okta* etc. using Nginx username passthrough

**When not to use it:** If you don't require local hosting, commercial cloud services may be more suitable and provide more features. Some networking and Linux experience is recommended for setup.

![Video listing screenshot](doc/video-list.webp)

## Demo

**Quick Start with Docker:**

- **Single-user demo:** No authentication
  ```bash
  docker run --rm -it -p 0.0.0.0:8080:80 -v clapshot-demo:/mnt/clapshot-data/data \
    elonen/clapshot:latest-demo
  ```
- **Multi-user demo:** With HTTP basic authentication
  ```bash
  docker run --rm -it -p 0.0.0.0:8080:80 -v clapshot-demo:/mnt/clapshot-data/data \
    elonen/clapshot:latest-demo-htadmin
  ```

Access the web UI at `http://127.0.0.1:8080`.

**User Management:** The basic auth version uses [htadmin](https://github.com/soster/htadmin) for user management. Default credentials are show in terminal.

These Docker images are demos only and _not_ meant for production. Here's a better way to deploy the system:

## Simplified Small-Business Deployment

For a simple production setup with password authentication on a Debian 12 host:

1. Prepare a Debian 12 host with a mounted block device (or just directory) at `/mnt/clapshot-data`.
2. Download [Clapshot Debian Bookworm Deployment Script](https://gist.github.com/elonen/80a721f13bb4ec1378765270094ed5d5) and and edit it to customize your access URL
3. Run the script as root to install and auto-configure Clapshot.

Change the default admin password and manage users in Htadmin as needed.

## Configuration and Operation

See the [Sysadmin Guide](doc/sysadmin-guide.md) for information on
- building and unit tests
- configuring Nginx reverse proxy (for HTTPS and auth)
- using *systemd* for process management
- performing database migrations
- implementing advanced authentication methods

## Development Setup

Follow the [development setup guide](doc/development-setup.md) . This includes setting up the server and client development environments and running local builds and tests.

## Contributions

Contributions are welcome, especially for features and improvements that benefit the wider user base. Please add your copyright notice for significant contributions.

## License and Copyrights

Clapshot is licensed under the GPL v3.
Copyright 2022, 2023 by Jarno Elonen