import json
from textwrap import dedent
import clapshot_grpc.proto.clapshot as clap
from .l10n import _

class ActiondefsHelper:
    def __init__(self):
        pass

    def make_custom_actions_map(self) -> dict[str, clap.ActionDef]:
        """
        All custom client actions for a user session, keyed by the name that PageItems reference
        (in popup_actions / open_action / media_file_added_action). Each is JS the client runs.
        """
        return {
            "new_folder": self.make_new_folder_action(),
            "move_to_parent": self.make_move_to_parent_action(),
            "on_media_file_added": self.make_on_media_file_added_action(),
            "share_folder": self.make_share_folder_action(),
            "copy_shared_link": self.make_copy_shared_link_action(),
            "revoke_share": self.make_revoke_share_action(),
            "cleanup_empty_user": self.make_cleanup_empty_user_action(),

            # Version set actions:
            "into_version_set": self.make_into_version_set_action(),
            "into_normal_folder": self.make_into_normal_folder_action(),
            "make_versioned": self.make_make_versioned_action(),
            "manage_versions": self.make_manage_versions_action(),
            "set_active_version": self.make_set_active_version_action(),
        }

    def make_new_folder_action(self) -> clap.ActionDef:
        # Listing-background popup ("New folder"): prompts for a name and creates a subfolder in the current folder.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("New folder"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-folder-plus", color=None)),
                key_shortcut=None,
                natural_desc=_("Create a new folder")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    var folder_name = (prompt({json.dumps(_("Name for the new folder"))}, ""))?.trim();
                    if (folder_name) {{ clapshot.callOrganizer("new_folder", {{name: folder_name}}); }}
                """).strip()))

    def make_move_to_parent_action(self) -> clap.ActionDef:
        # Per-item popup (shown when not in the root): moves the selected item(s) up into the parent folder.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Move to parent"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-arrow-turn-up", color=None)),
                key_shortcut=None,
                natural_desc=_("Move item to parent folder")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var listingData = _action_args.listing_data;
                    var items = _action_args.selected_items;

                    if (!listingData.parent_folder_id) {
                        alert("parent_folder_id missing from listingData.");
                        return;
                    }
                    var folderId = listingData.parent_folder_id;
                    var ids = clapshot.itemsToIDs(items);
                    clapshot.moveToFolder(folderId, ids, listingData);
                """).strip()))

    def make_on_media_file_added_action(self) -> clap.ActionDef:
        # Not a popup: the callback the client runs after a file finishes uploading; moves it into the current folder.
        return clap.ActionDef(
            ui_props=None,  # not an UI action, just a callback
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var mfid = _action_args.media_file_id;
                    var listingData = _action_args.listing_data;
                    var folderId = listingData?.folder_id;

                    if (!folderId || !mfid) {
                        var msg = "on_media_file_added error: media_file_id missing, or folder_id from listingData.";
                        alert(msg); console.error(msg);
                    } else {
                        clapshot.moveToFolder(folderId, [{mediaFileId: mfid}], listingData);
                    }
                """).strip()))

    def make_share_folder_action(self) -> clap.ActionDef:
        # Per-folder popup (owner, not yet shared): creates a public share link for the folder.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Share folder"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-share-nodes", color=None)),
                key_shortcut=None,
                natural_desc=_("Create a shareable link to this folder")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {{
                        alert({json.dumps(_("No folder selected to share"))});
                        return;
                    }}
                    clapshot.callOrganizer("share_folder", {{id: folderId}});
                """).strip()))

    def make_revoke_share_action(self) -> clap.ActionDef:
        # Per-folder popup (owner, shared): revokes the folder's share link.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Stop sharing"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-link-slash", color=None)),
                key_shortcut=None,
                natural_desc=_("Revoke the shared link for this folder")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {{
                        alert({json.dumps(_("No folder selected to unshare"))});
                        return;
                    }}
                    if (confirm({json.dumps(_("Are you sure you want to revoke the shared link for this folder?"))})) {{
                        clapshot.callOrganizer("revoke_share", {{id: folderId}});
                    }}
                """).strip()))

    def make_copy_shared_link_action(self) -> clap.ActionDef:
        # Per-folder popup (owner, shared): copies the folder's share URL to the clipboard.
        # Pre-computed outside the f-string below: its backslash escapes (\n\n) are not
        # allowed inside an f-string expression part on Python < 3.12 (PEP 701).
        copied_msg = json.dumps(_(
            "Shared link copied to clipboard!\n\nNOTE: Sharing a folder reveals direct "
            "links to all files currently in it, effectively giving recipient PERMANENT "
            "access to them, even if remove the folder share later."))
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Copy URL"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-copy", color=None)),
                key_shortcut=None,
                natural_desc=_("Copy the shared link to clipboard")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    var sharedFolderTokens = JSON.parse(_action_args.listing_data?.shared_folder_tokens || '{{}}');
                    var shareToken = sharedFolderTokens[folderId];

                    if (!shareToken) {{
                        alert({json.dumps(_("No shared link available for this folder"))});
                        return;
                    }}

                    // Construct the share URL using the current page's origin
                    var shareUrl = window.location.origin + "/?p=shared." + shareToken;

                    if (navigator.clipboard && navigator.clipboard.writeText) {{
                        navigator.clipboard.writeText(shareUrl).then(function() {{
                            alert({copied_msg});
                        }}).catch(function() {{
                            prompt({json.dumps(_("Copy this shared link:"))}, shareUrl);
                        }});
                    }} else {{
                        prompt({json.dumps(_("Copy this shared link:"))}, shareUrl);
                    }}
                """).strip()))

    def make_into_version_set_action(self) -> clap.ActionDef:
        # Per-folder popup (eligible normal folders): converts the folder into a version set.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Into Version Set"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-layer-group", color=None)),
                key_shortcut=None,
                natural_desc=_("Turn this folder into a version set (revisions of one media file)")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    // Convert right-clicked folder to a version set.
                    var folder = _action_args.selected_items?.[0]?.folder;
                    if (!folder?.id) {{ alert({json.dumps(_("No folder selected"))}); return; }}
                    clapshot.callOrganizer("set_folder_kind", {{id: parseInt(folder.id), kind: "version_set"}});
                """).strip()))

    def make_into_normal_folder_action(self) -> clap.ActionDef:
        # Per-tile popup (version sets): reverts a version set back to a normal folder.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Into Normal Folder"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-folder", color=None)),
                key_shortcut=None,
                natural_desc=_("Turn this version set back into a normal folder")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    // Revert the right-clicked version set to a normal folder.
                    var folder = _action_args.selected_items?.[0]?.folder;
                    if (!folder?.id) {{ alert({json.dumps(_("No folder selected"))}); return; }}
                    clapshot.callOrganizer("set_folder_kind", {{id: parseInt(folder.id), kind: "normal"}});
                """).strip()))

    def make_make_versioned_action(self) -> clap.ActionDef:
        # Per-media-file(s) popup: groups the selected media files into a new version set.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Make Versioned"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-layer-group", color=None)),
                key_shortcut=None,
                natural_desc=_("Group the selected media files into a new version set")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    // Group the selected media files into a new version set (server rejects any folders).
                    var items = _action_args.selected_items || [];
                    if (!items.length) {{ alert({json.dumps(_("No media files selected"))}); return; }}
                    clapshot.callOrganizer("make_versioned", {{ids: clapshot.itemsToIDs(items)}});
                """).strip()))

    def make_manage_versions_action(self) -> clap.ActionDef:
        # Per-tile popup (version sets): opens the set's "Manage Versions" listing.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Manage Versions"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-list-ol", color=None)),
                key_shortcut=None,
                natural_desc=_("Open the version set to manage its versions")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    // Open the version set's folder (its "Manage Versions" listing).
                    var folder = _action_args.selected_items?.[0]?.folder;
                    if (!folder?.id) {{ alert({json.dumps(_("No version set selected"))}); return; }}
                    clapshot.callOrganizer("open_folder", {{id: parseInt(folder.id)}});
                """).strip()))

    def make_set_active_version_action(self) -> clap.ActionDef:
        # Per-version popup inside a set (the player header <select> calls set_active_version directly):
        # makes the selected version the set's active one.
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Set Active Version"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-circle-check", color=None)),
                key_shortcut=None,
                natural_desc=_("Make this version the active one for the set")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    // Mark the selected version as the set's active version.
                    var it = _action_args.selected_items?.[0];
                    var mfid = it?.mediaFile?.id;
                    var folderId = _action_args.listing_data?.folder_id;
                    if (!mfid || !folderId) {{ alert({json.dumps(_("Cannot set active version (missing ids)"))}); return; }}
                    clapshot.callOrganizer("set_active_version", {{folder_id: parseInt(folderId), media_file_id: mfid}});
                """).strip()))

    def make_cleanup_empty_user_action(self) -> clap.ActionDef:
        # Admin per-user-folder popup (+ the "delete all empty users" button): deletes a user that has
        # no media files and only an empty root folder.
        # Pre-computed outside the f-string below: its backslash escapes (\n\n) are not
        # allowed inside an f-string expression part on Python < 3.12 (PEP 701).
        confirm_msg = json.dumps(_(
            "This will delete the user if they have no media files and only an empty root "
            "folder.\n\nComments from this user will be preserved but marked as from a "
            "deleted user.\n\nAre you sure?"))
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label=_("Delete user"),
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-user-minus", color=clap.Color(r=220, g=38, b=38))),
                key_shortcut=None,
                natural_desc=_("Delete user if they have no content (only empty root folder)")),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent(f"""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {{
                        alert({json.dumps(_("No user folder selected for cleanup"))});
                        return;
                    }}
                    if (confirm({confirm_msg})) {{
                        clapshot.callOrganizer("cleanup_empty_user", {{folder_id: folderId}});
                    }}
                """).strip()))
