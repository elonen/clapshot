"""
Example metaplugin: "Move to folder ID" popup action.

Adds a right-click action to folders and media files that moves ALL currently
selected items into a folder identified by its numeric ID -- exactly as if they
had been drag & dropped onto that folder.

This is a power-user tool: instead of a folder picker, you paste the destination
folder's ID. Open the target folder in the browser and copy the ID from the
address bar (the `?p=...` value, e.g. `1.5.23`); the deepest number is the folder.

It reuses the server's normal move pipeline via `clapshot.moveToFolder(...)`, so
all the usual rules apply automatically (ownership/authorization checks,
version-set handling, source-folder refresh, ownership transfer, etc.). No custom
organizer command is needed -- the action is pure client-side JavaScript.
"""

import logging
from logging import Logger, LoggerAdapter
from textwrap import dedent
from typing import Optional

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org

from organizer.metaplugin import OrganizerContext, FolderContext, MetaPluginInterface

try:
    from typing import override  # type: ignore   # Python 3.12+
except ImportError:
    def override(func):  # type: ignore
        return func


class Plugin(MetaPluginInterface):
    """Adds a "Move to folder ID" popup action to every item in a folder listing."""

    PLUGIN_NAME = "move_to_folder_id"
    PLUGIN_VERSION = "1.0.0"

    # Set to True to only show this action for admins. To restrict to specific
    # users instead, compare session.user.id in augment_folder_listing() (see the
    # TARGET_USER_ID pattern in calculate_sha256.py).
    ADMIN_ONLY = False

    def __init__(self) -> None:
        self.ctx: Optional[OrganizerContext] = None
        self.log: Logger | LoggerAdapter = logging.getLogger(self.PLUGIN_NAME)

    @override
    async def on_init(self, context: OrganizerContext) -> None:
        self.ctx = context
        self.log = context.log
        self.log.info(f"{self.PLUGIN_NAME} v{self.PLUGIN_VERSION} initialized")

    @override
    def extend_actions(self, actions: dict[str, clap.ActionDef]) -> dict[str, clap.ActionDef]:
        # The action is registered globally here; augment_folder_listing() decides
        # which items actually get it in their popup menu.
        actions["move_to_folder_id"] = clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Move to folder ID",
                icon=clap.Icon(fa_class=clap.IconFaClass(
                    classes="fa fa-right-to-bracket",
                    color=clap.Color(r=120, g=170, b=230))),
                key_shortcut=None,
                natural_desc="Move the selected item(s) into a folder given by its numeric ID"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(r"""
                    // Move every selected item into a folder identified by its ID,
                    // as if drag & dropped there. `dst_folder_id` for moveToFolder()
                    // is the folder's numeric DB id (the deepest number in the
                    // browser address bar's ?p=... path).
                    var items = _action_args.selected_items || [];
                    if (!items.length) { alert("Nothing selected to move."); return; }

                    var raw = prompt(
                        "Move " + items.length + " item(s) to which folder?\n\n" +
                        "Open the destination folder, then copy the folder ID from the " +
                        "browser address bar (the part after ?p= , e.g. 1.5.23) and paste it here:",
                        "");
                    if (raw === null) return;          // cancelled
                    raw = raw.trim();
                    if (!raw) return;

                    // Accept a pasted URL, a dotted folder path (root.sub.sub), or a bare ID.
                    var m = raw.match(/[?&]p=([^&#]+)/);
                    var path = m ? decodeURIComponent(m[1]) : raw;
                    var parts = path.split(".");
                    var dst = parts[parts.length - 1].trim();   // destination = deepest folder in the path
                    if (!/^\d+$/.test(dst)) { alert("Could not read a folder ID from:\n" + raw); return; }

                    clapshot.moveToFolder(dst, clapshot.itemsToIDs(items), _action_args.listing_data || {});
                """).strip()))

        return actions

    @override
    async def augment_folder_listing(
        self, listing_items: list[clap.PageItemFolderListingItem], folder_context: FolderContext, session: org.UserSessionData
    ) -> list[clap.PageItemFolderListingItem]:
        if self.ADMIN_ONLY and not session.is_admin:
            return listing_items

        # Attach the action to every folder / media file so it's available whenever
        # the selection contains at least one such item.
        for item in listing_items:
            if (item.folder and item.folder.id) or (item.media_file and item.media_file.id):
                if "move_to_folder_id" not in item.popup_actions:
                    item.popup_actions.append("move_to_folder_id")

        return listing_items
