# Clapshot Slack Unfurl Bot

A Slack app that unfurls [Clapshot](https://github.com/elonen/clapshot) links posted in Slack channels, showing video thumbnails, metadata, and comment drawings as rich previews.

## Features

- **Video unfurls**: thumbnail, title, duration, owner, upload date
- **Comment unfurls**: drawing annotation (or video thumbnail fallback), comment text, timecode, commenter
- **Channel allowlist**: control which channels get unfurls via channel IDs and name regexes
- **LAN-friendly**: runs co-located with Clapshot, reads files and SQLite directly — no public endpoint needed
- **Socket Mode**: connects outbound to Slack, works behind firewalls and NAT
- **Image upload**: uses Slack's `slack_file` for thumbnails — no image proxy required

## Setup

### 1. Create a Slack App

1. Go to [api.slack.com/apps](https://api.slack.com/apps) and create a new app (or use `slack-app-manifest.json`)
2. Enable **Socket Mode** (Settings → Socket Mode) and generate an App-Level Token with `connections:write` scope — save the `xapp-...` value (used to authenticate the WebSocket connection that receives events)
3. Add **Event Subscriptions** → Subscribe to bot event: `link_shared`
4. Register your Clapshot domain(s) under **Event Subscriptions → App Unfurl Domains**
5. Add OAuth scopes: `links:read`, `links:write`, `chat:write`, `files:write`, `channels:read`, `groups:read`, `mpim:read`, `im:read`
6. Install the app to your workspace, then copy the Bot Token (`xoxb-...`) from **OAuth & Permissions**

### 2. Configure

```bash
cp doc/clapshot-slack-unfurl.toml.example clapshot-slack-unfurl.toml
# Edit paths and allowed_channels, set tokens via env vars:
export SLACK_BOT_TOKEN="xoxb-..."
export SLACK_APP_TOKEN="xapp-..."
```

The Debian package installs a default config to `/etc/clapshot-slack-unfurl.toml`.

### 3. Run

```bash
# From source
python -m clapshot_slack_unfurl.main

# Or if installed
clapshot-slack-unfurl
```

Options: `--config PATH`, `--debug`, `--log PATH`

## Development

```bash
python3 -m venv _venv
_venv/bin/pip install -r requirements.txt -r requirements-dev.txt
_venv/bin/pip install -e .
_venv/bin/pytest tests/ -v
```

## Debian Package

```bash
make debian-docker  # builds .deb in dist_deb/
```

## License

MIT
