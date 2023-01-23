# Clapshot - self hosted video review tool

[![Build Status](https://app.travis-ci.com/elonen/clapshot.svg?branch=master)](https://app.travis-ci.com/elonen/clapshot)
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

Version 0.4.0 has _collaborative review sessions_, where playback controls and drawings
are mirrored in real-time to all participants. It's meant to supplement remote video conferences
such as Google Meets that don't play video well over screen sharing. Click the "head plus" icon
in page header to start it.

![Video listing screenshot](doc/video-list.webp)


### Demo

To try out Clapshot using Docker, either run
```bash
docker run --rm -it -p 0.0.0.0:8080:80 -v clapshot-demo:/mnt/clapshot-data/data \
  elonen/clapshot:0.4.0-demo
```
...to download a demo image from Docker Hub, or check out the repo and run `make run-docker` to build it manually.

Once the server is running, open your browser at http://127.0.0.1:8080

## Deployment in production

The server is started with command `clapshot-server`. It stays in the
foreground, and should therefore be started by a process manager such as systemd.

Preferred deployment and upgrade method is to install server and client as Debian
packages. Whereas `clapshot-server` is a foreground binary that is configured with command line options,
the Debian package contains a systemd service file that demonizes it, and config file `/etc/clapshot-server.conf` that is
translated into the appropriate CLI options automatically. 

Server should be put behind a reverse proxy in production, but
can be developed and tested without one. The server .deb package contains
an example Nginx config file (`/usr/share/doc/clapshot-server/examples/`) that

 1. reverse proxies the server API (websocket),
 2. serves out frontend files (.html .js .css),
 3. serves uploaded video files from `videos/` directory, and
 4. contains examples on how to add HTTPS and authentication

Clapshot server itself contains no authentication code. Instead, it trusts
HTTP server (reverse proxy) to take care of that (e.g. by Kerberos or HTTP basic-auth) and
to pass authenticated user ID and username in request headers. See the Nginx example conf file for details.
There's currently no authorization (that is, all authenticated users are assumed to have same privileges). 

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


## Development status

Clapshot is usable but not very polished or feature rich. It was started
and is currently being maintained / developed for a specific project, so releases for
the general public and generic use are not a priority. That said, feel free to try
if it fits your use case and to contribute in development.

## License

Clapshot is licensed under GPL v3, (c) 2022, 2023 by Jarno Elonen
