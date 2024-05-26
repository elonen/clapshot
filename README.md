# Clapshot: Self-Hosted Video/Media Review Tool
[![Release](https://img.shields.io/github/v/release/elonen/clapshot?include_prereleases)]() [![Build and test](https://github.com/elonen/clapshot/actions/workflows/docker-test.yml/badge.svg)](https://github.com/elonen/clapshot/actions/workflows/docker-test.yml)

## Overview

Clapshot is an open-source, self-hosted tool for collaborative video/media review and annotation. It features a Rust-based API server and a Svelte-based web UI. This tool is ideal for scenarios requiring local hosting of videos due to:

1. Policy constraints (*enterprise users*), or
2. Cost-benefit concerns against paid cloud services (*very small businesses*)

![Review UI screenshot](doc/video-commenting.webp)

### Key Features

- Media file ingestion via HTTP uploads or shared folders
- Media (video, audio, image) file transcoding with FFmpeg
- Commenting, drawing annotations, and threaded replies
- Real-time collaborative review sessions
- Stores media files on disk and metadata in an SQLite (3.5+) database
- Authentication agnostic, you can use *OAuth, JWS, Kerberos, Okta* etc., using Nginx username pass-through
- **[NEW]** Extensible "Organizer" plugins for custom integrations, workflow, and access control

### When not to use Clapshot

If you don't require local hosting, or are not adept in networking and Linux, consider commercial cloud services which may offer more user-friendly interfaces and additional features out of the box.

![Video listing screenshot](doc/video-list.webp)

## Demo

**Quick Start with Docker:**

- **Single-user demo:** No authentication

```bash
docker run --rm -it -p 0.0.0.0:8080:80 -v clapshot-demo:/mnt/clapshot-data/data elonen/clapshot:latest-demo
```

- **Multi-user demo** with HTTP basic auth, append `-htadmin`, i.e.: `... elonen/clapshot:latest-demo-htadmin`

After the Docker image starts, access the web UI at `http://127.0.0.1:8080`.

The basic auth multi-user demo uses [PHP htadmin](https://github.com/soster/htadmin) for user management. Default credentials are shown in the terminal.

These Docker images are demos only and *not meant for production*.

Here’s a better way to deploy the system:

## Simplified Small-Business Deployment

For a simple production setup with password authentication on a Debian 12 host:

1. Prepare a Debian 12 with a mounted block device (or just directory) at `/mnt/clapshot-data`.
2. Download [Clapshot Debian Bookworm Deployment Script](https://gist.github.com/elonen/80a721f13bb4ec1378765270094ed5d5)
3. Run the script as root to install and auto-configure Clapshot.
4. **!! Change the default `admin` and `htadmin` passwords, and delete example users in Htadmin !!**

## Configuration and Operation

See the [Sysadmin Guide](doc/sysadmin-guide.md) for information on:

- configuring Nginx reverse proxy (for HTTPS and auth)
- using *systemd* for process management
- performing database migrations
- implementing advanced authentication methods
- building manually and running unit tests

See [Upgrading Guide](doc/upgrading.md) for instructions on installing a new release over an old one.

## Architecture Overview

Main components:

- **Clapshot Client** – Single Page Application (SPA) that runs in the browser. Connects to Clapshot Server via Websocket. Written in *Svelte*.
- **Clapshot Server** – Linux daemon that handles most server-side logic. Binary written in *Rust*. Listens on `localhost` to the reverse proxy for plaintext HTTP and WSS.
- **Clapshot Organizer(s)** – Plugin(s) that organize media files into a custom folder hierarchy, etc. Written in Python (or any other language). See below for details.

Production deployments also depend on:

- **Web Browser** – Chrome works best. Loads and shows the Client.
- **Nginx Web Server** – SSL reverse proxy between Client and Server + static asset delivery for browser. Also routes session auth to Authentication Proxy.
- **Authentication Proxy** – Any auxilliary HTTP daemon that authenticates users and return a **user id** and **username** in HTTP headers. In the demo, this is `/var/www/.htpasswd` + [PHP htadmin](https://github.com/soster/htadmin), but you can also use combinations like [Okta](https://www.okta.com/) + [Vouch](https://github.com/vouch/vouch-proxy) + [LDAP Authz Proxy](https://github.com/elonen/ldap_authz_proxy) or something equally advanced.

- **Sqlite DB** – Stores metadata, comments, user messages etc. Both Clapshot Server and Organizer(s) access this. This is just a file, not a daemon.
- **ffmpeg** and **mediainfo** – Clapshot Server processes media files with these commands.
- **File System** – Media files, HTML, JavaScript, CSS, thumbnail images etc, also `clapshot.sqlite`.

See [sequence diagram](doc/generated/open-frontpage-process.svg) for details on how these interact when a user opens the main page.

## Organizer Plugin System (New in 0.6.0):
Clapshot now includes an extensible [Organizer Plugin system](doc/organizer-plugins.md). Organizer can implement custom UIs, virtual folders, enforce access control based on your business logic, and integrate with existing systems (LDAP, project management databases, etc).

Organizers use gRPC to communicate with the Clapshot Server, and can be implemented in any language.

The provided default/example organizer, called “[basic_folders](organizer/basic_folders/README.md)” (in *Python*), implements:
 - personal folders for users, and
 - for admin, a list of users and a way to manage their folder contents.

### Work In Progress

The [Organizer API](protobuf/proto/organizer.proto) is still evolving, so you are invited to **provide feedback** and discuss future development. However, please **do not expect backward compatibility** for now.

## Development Setup

The [development setup guide](doc/development-setup.md) covers setting up the server and client development environments, and running local builds and tests.

## Contributions

Contributions are welcome, especially for features and improvements that benefit the wider user base. Please add your copyright notice for significant contributions.

## Licensing

Copyright 2022 – 2024 by Jarno Elonen

- Clapshot Server and Client are licensed under the **GNU General Public License, GPLv2**.
- gRPC/proto3 libraries and example organizer plugins are under the **MIT License**.

This split licensing allows you to implement proprietary UIs and workflows through custom Organizer plugins without releasing them to the public.
