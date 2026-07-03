"""Entry point: Slack app setup, Socket Mode, event wiring."""

import logging
from typing import Any

from slack_bolt import App
from slack_bolt.adapter.socket_mode import SocketModeHandler
from slack_sdk import WebClient

from . import config as cfg
from . import log as logsetup
from .acl import is_channel_allowed
from .slack import resolve_channel_name, upload_image, send_unfurl, send_ephemeral
from .unfurl import build_unfurl

log = logging.getLogger(__name__)


def main() -> None:
    conf = cfg.load()
    logsetup.setup(log_path=conf.log_path, debug=conf.debug)

    app = App(token=conf.slack_bot_token)

    @app.event("link_shared")
    def handle_link_shared(event: dict[str, Any], client: WebClient, context: dict[str, Any]) -> None:
        channel: str = event.get("channel", "")
        user: str = event.get("user", "")
        ts: str = event.get("message_ts", "")

        # Guard against self-posted links in case future extensions make the bot
        # post messages with Clapshot URLs.
        if user == context.get("bot_user_id"):
            return

        # Composer previews (typing, not yet sent) — skip to avoid
        # unfurling before the message is actually posted.
        if event.get("source") == "composer":
            return

        # ACCESS CONTROL: check channel against the allowlist.
        # Deny early and notify the user via ephemeral message.
        channel_name = resolve_channel_name(client, channel)
        if not is_channel_allowed(channel, channel_name, conf.allowed_channels):
            log.info("Denied unfurl in channel %s (%s) for user %s",
                     channel, channel_name or "?", user)
            send_ephemeral(client, channel, user,
                           "Clapshot link unfurling is not enabled in this channel.")
            return

        # Process each matched link in the message.
        links: list[dict[str, Any]] = event.get("links", [])
        for link in links:
            url: str = link.get("url", "")

            # Look up video/comment in Clapshot's DB and build the
            # unfurl payload (title, description, image).
            # Returns None on missing data or errors (silent skip + log).
            result = build_unfurl(url, conf.sqlite_path, conf.videos_dir,
                                  image_min_width=conf.image_min_width,
                                  image_max_width=conf.image_max_width)
            if not result:
                log.info("No unfurl for %s (not found or error)", url)
                continue

            log.info("Unfurling %s → %s (image: %s)",
                     url, result.title, result.image_filename if result.image_bytes else "none")

            # Upload thumbnail/drawing to Slack as a private file.
            # Cache keyed on source path + mtime to avoid re-uploads.
            file_id: str | None = None
            if result.image_bytes:
                src = result.image_source_path
                mtime = src.stat().st_mtime if src and src.exists() else 0.0
                file_id = upload_image(client, result.image_bytes,
                                       result.image_filename,
                                       result.image_filename, mtime)

            # Attach the unfurl (Block Kit blocks) to the original message.
            send_unfurl(client, channel, ts, url,
                        result.title, result.fields, result.text, file_id)

    handler = SocketModeHandler(app, conf.slack_app_token)
    log.info("Starting Clapshot Slack unfurler (Socket Mode)")
    handler.start()


if __name__ == "__main__":
    main()
