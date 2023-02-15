# Clapshot - self hosted video review tool

[![Build and test](https://github.com/elonen/clapshot/actions/workflows/docker-test.yml/badge.svg)](https://github.com/elonen/clapshot/actions/workflows/docker-test.yml)
[![Release](https://img.shields.io/github/v/release/elonen/clapshot?include_prereleases)]()

## Introduction

Clapshot is a self-hosted, web-based, open source video review/annotation tool.
It consists of an API server (written in Rust) and a web UI (written in Svelte).

![Review UI screenshot](doc/video-commenting.webp)

Users can submit videos by HTTP upload or by copying them to `incoming` directory (e.g. via Samba).
If bitrate exceeds configured target or codec/container is not recognized as supported (guaranteed to be viewable in a browser),
server transcodes the video with FFMPEG.

After a video is ingested succesfully, users can view the file, add comments, draw annotations
and reply to each other's comments. Videos are stored on disk as files, while metadata and comments
go to an Sqlite 3.5+ database file.

Clapshot supports _collaborative review sessions_, where playback controls and drawings
are mirrored in real-time to all participants. It's meant to supplement remote video conferences
such as Google Meets that don't play video well over screen sharing. Click the "head plus" icon
in page header to start it.

![Video listing screenshot](doc/video-list.webp)


### Demo

To try out Clapshot using Docker, you can either download a demo image from Docker Hub, or clone the repo and run `make run-docker` to build it manually.

Once the server is running, open your browser at http://127.0.0.1:8080

#### Single-user demo (no authentication)

```bash
docker run --rm -it -p 0.0.0.0:8080:80 -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo
```

#### Multi-user demo (with HTTP basic authentication)

```bash
docker run --rm -it -p 0.0.0.0:8080:80 -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:latest-demo-htadmin
```

The _basic auth_ version uses https://github.com/soster/htadmin as a simple user management tool.
You can access it at http://127.0.0.1:8080/htadmin/ and create users there (username `htadmin`, password `admin`), or just use the default ones: `demo`/`demo`, `alice`/`alice123` and `admin`/`admin`.

Note that each user only sees their own videos in their front page, but can still review and comment on videos uploaded by other users. Users are supposed to share the video URL with each other to collaborate,
perhaps by using a chat tool such as Slack, issue tracker or email.

#### Advanced authentications

Clapshot server itself contains no authentication code. Instead, it trusts
HTTP server (reverse proxy) to take care of that and to pass authenticated user ID
and username in request headers. This is exactly what the basic auth / htadmin demo
above does, too.

Most modern real-world deployments will likely use some more advanced authentication mechanism, such as OAuth, Kerberos etc, but htadmin is a good starting point.

See [/Dockerfile](Dockerfile), 
[/test/docker-entry_htadmin.sh](test/docker-entry_htadmin.sh) and 
[client/debian/additional_files/clapshot+htadmin.nginx.conf](client/debian/additional_files/clapshot+htadmin.nginx.conf) for details on how the integration works.

Authorization is also supposed to be handled on web server, at least for now.
See for example https://github.com/elonen/ldap_authz_proxy on how to
authorize users against Active Directory/LDAP using Nginx. I wrote it complement
Nginx spnego authn, which uses Kerberos and thus doesn't have a concept of access groups.
If you want to use Kerberos, you can also check out https://github.com/elonen/debian-nginx-spnego
on how to build .deb packages for it.

There's currently no demos for any of these more advanced auths (`vouch-proxy` example for Okta etc. would be especially welcome, if you want to contribute!), but I've done an Active Directory Kerberos+LDAP deployment, see .

## Deployment in production

The server is started with command `clapshot-server`. It stays in the
foreground, and should therefore be started by a process manager such as systemd.

Preferred deployment and upgrade method is to install server and client as Debian
packages. Whereas `clapshot-server` is a foreground binary that is configured with command line options,
the Debian package contains a systemd service file that demonizes it, and config file `/etc/clapshot-server.conf` that is translated into the appropriate CLI options automatically. 

Server should be put behind a reverse proxy in production, but
can be developed and tested without one. The server .deb package contains
an example Nginx config file (`/usr/share/doc/clapshot-server/examples/`) that

 1. reverse proxies the server API (websocket),
 2. serves out frontend files (.html .js .css),
 3. serves uploaded video files from `videos/` directory, and
 4. contains examples on how to add HTTPS and authentication

While the server uses mostly Websocket, there's a `/api/health` endpoint that can be used
for monitoring. It returns 200 OK if the server is running.

## Database upgrades

Some releases require database migrations. If you're upgrading from a previous version, **make a backup of your database** (`clapshot.sqlite`) and then either add line `migrate = true` to `/etc/clapshot-server.conf` (Debian package) or use `--migrate` option when running the server manually.

Once the server is started with migrate enabled, it will run database migrations
on startup. After that, you can remove the `migrate` option and restart the server.

Running the server without migrations enabled will detect that the database is out of date, log an error and exit.

## Building

The recommended way to build Clapshot is to use Docker and the provided Makefile:

 1. Install and start Docker
 2. At the top level (not "server/" or "client/"), run `make debian-docker`

This will build Debian packages and put them in the `dist_deb/` directory.

You can also build everything directly on your system, but Docker
is cleaner and doesn't require installing extra dependecies.
See Makefiles and Dockerfiles for details.

## Running tests

Use `make test` at top level to run all tests in a Docker container.

During development, running unit tests locally for clapshot-server can be more
convenient. To do so, install Rust + Cargo and issue `cd server; make test-local`.

## Running the server

Although .deb packages and the bundled config file are the recommended way to run
Clapshot in production (`systemctl start clapshot-server.service`), it is also possible to
run the server directly from command line and have all logging go to stdout.
This is useful for development and debugging. Call `clapshot-server --help` to show startup options.

## Development setup

This is my current development setup (in Feb 2023). Adapt to your own needs.

 * Windows with WSL2, running Debian Bullseye
 * Docker (running inside the WSL2 Debian)

First, open two WSL2 Debian terminals open - one for server dev (Rust), one for client dev (Svelte). Then:

Server:

 * install rustup 2021 stable toolchain
 * `cd server`
 * `code .` to open VS Code
 * `make run-local` - builds server, then listens on port 8089, logs to stdout
 * (optional) `make test-local` to run server unit/integration tests without client

Client:

  * `code .` to open VS Code (at top level)
  * Click "reopen in container" button (to avoid installing Node.js and packages locally)
  * Open a terminal in VS Code (runs inside the dev container)
    - `cd client`
    - `npm install` to install dependencies
    - `npm run dev` to start dev HTTP on port 5173
  * Open http://localhost:5173/ in browser. Vite will reflect changes to the code
    while developing the Svete app, which is very handy. The client will connect Websocket to `ws://localhost:8089/` by default, so you can see what the server is doing in the other WSL terminal.

When done, at top level, run one of the following:

 * `make test` to build both client and server, and to run all tests in a pristine Docker container
 * `make debian-docker` to test and also build .debs (stored in `dist_deb/`)
 * `make run-docker` to automatically build .debs, install them in a Docker container, run server as a systemd service + client as a Nginx site inside it. This is the recommended way to test the whole stack
 * `make build-docker-demo` to build a demo image (like the one that can be found in Docker Hub)

## Contributions

Clapshot was started and is currently being maintained / developed for a specific project,
so features/releases for generic use may not always be a priority.

That said, feel free to try if it fits your use case and to contribute in development.

For pull requests that introduce significant new code or other materials, please add your
name and contribution year to the copyright notices.

## License and copyrights

Clapshot is licensed under GPL v3
Copyright 2022, 2023 by Jarno Elonen
