# Clapshot Server

This is the server component for Clapshot, an open-source, self-hosted tool designed for collaborative video/media review and annotation. For a comprehensive overview and detailed documentation, please visit the [GitHub project page](https://github.com/elonen/clapshot).

## Overview

Clapshot Server is a Rust-based HTTP/Websocket daemon that manages the server-side logic for Clapshot. It is designed to run behind a reverse proxy and is typically managed using systemd or another process manager.

## Configuration and Operation

After installation on a Debian-based system, configure Clapshot Server by editing the configuration file at `/etc/clapshot-server.conf`.

For available startup options, use `clapshot-server --help`. All options listed by `--help` can be used in the config file. In fact, a script at `/usr/share/clapshot-server/run-with-conf.sh` will convert the config file into CLI options upon startup.

### Running the Server

The server runs as a systemd service by default. To start, stop, or check the status of the Clapshot Server, use the following commands:

```bash
sudo systemctl start clapshot-server
sudo systemctl stop clapshot-server
sudo systemctl status clapshot-server
```

After installation, enable auto start by `sudo systemctl enable clapshot-server`.

### Data directories

Video files and an Sqlite database are by default stored in `/mnt/clapshot-data/`, but the location can be changed in config file.

### Log Files

Log files for the Clapshot Server can be found at `/var/log/clapshot.log`.
When debugging, also take a look at `sudo systemctl status clapshot-server` in case the startup failed before writing anything to the log file. Setting `debug = true` in the config file will increase log verbosity.

## Database Upgrades

When installing a new version over an existing system, make sure `migrate = true` is set in `/etc/clapshot-server.conf`.
Keeping it there permanently should be safe in current versions, as the server will back up the database in a .tar.gz before applying migrations.

## External dependencies

Most deployments will also run an Nginx instance, that reverse proxies API calls to the Clapshot Server, and serves Clapshot Client .html and .js files to the web browsers.

### Clapshot Client

Clapshot Client is a Svelte-based Single Page Application (SPA) that runs in the browser and connects to the Clapshot Server via WebSocket. It provides the user interface for video review and annotation.

### Clapshot Organizer

Clapshot Organizer is a plugin system that enables custom video organization, access control, and workflow enhancements. The included `basic_folders` plugin organizes videos into a hierarchical folder structure.

Organizer communicates with Clapshot Server using gRPC, over Unix Sockets by default. The server initiates a connection with the plugin, which remains active as long as the server is running.
