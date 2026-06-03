from textwrap import dedent
import clapshot_grpc.proto.clapshot as clap

class ActiondefsHelper:
    def __init__(self):
        pass

    def make_custom_actions_map(self) -> dict[str, clap.ActionDef]:
        """
        Popup actions for when the user right-clicks on a listing background.
        """
        return {
            "new_folder": self.make_new_folder_action(),
            "move_to_parent": self.make_move_to_parent_action(),
            "on_media_file_added": self.make_on_media_file_added_action(),
            "share_folder": self.make_share_folder_action(),
            "copy_shared_link": self.make_copy_shared_link_action(),
            "revoke_share": self.make_revoke_share_action(),
            "cleanup_empty_user": self.make_cleanup_empty_user_action(),
            "set_user_email": self.make_set_user_email_action(),
        }

    def make_new_folder_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="New folder",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-folder-plus", color=None)),
                key_shortcut=None,
                natural_desc="Create a new folder"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder_name = (prompt("Name for the new folder", ""))?.trim();
                    if (folder_name) { clapshot.callOrganizer("new_folder", {name: folder_name}); }
                """).strip()))

    def make_move_to_parent_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Move to parent",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-arrow-turn-up", color=None)),
                key_shortcut=None,
                natural_desc="Move item to parent folder"),
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
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Share folder",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-share-nodes", color=None)),
                key_shortcut=None,
                natural_desc="Create a shareable link to this folder"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {
                        alert("No folder selected to share");
                        return;
                    }
                    clapshot.callOrganizer("share_folder", {id: folderId});
                """).strip()))

    def make_revoke_share_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Stop sharing",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-link-slash", color=None)),
                key_shortcut=None,
                natural_desc="Revoke the shared link for this folder"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {
                        alert("No folder selected to unshare");
                        return;
                    }
                    if (confirm("Are you sure you want to revoke the shared link for this folder?")) {
                        clapshot.callOrganizer("revoke_share", {id: folderId});
                    }
                """).strip()))

    def make_copy_shared_link_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Copy URL",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-copy", color=None)),
                key_shortcut=None,
                natural_desc="Copy the shared link to clipboard"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    var sharedFolderTokens = JSON.parse(_action_args.listing_data?.shared_folder_tokens || '{}');
                    var shareToken = sharedFolderTokens[folderId];

                    if (!shareToken) {
                        alert("No shared link available for this folder");
                        return;
                    }

                    // Construct the share URL using the current page's origin
                    var shareUrl = window.location.origin + "/?p=shared." + shareToken;

                    if (navigator.clipboard && navigator.clipboard.writeText) {
                        navigator.clipboard.writeText(shareUrl).then(function() {
                            alert('Shared link copied to clipboard!\\n\\nNOTE: Sharing a folder reveals direct links to all files currently in it, effectively giving recipient PERMANENT access to them, even if remove the folder share later.');
                        }).catch(function() {
                            prompt('Copy this shared link:', shareUrl);
                        });
                    } else {
                        prompt('Copy this shared link:', shareUrl);
                    }
                """).strip()))

    def make_set_user_email_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Set email",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-envelope", color=None)),
                key_shortcut=None,
                natural_desc="Set email address for reply notifications"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderTitle = folder?.title || "";
                    var userId = folderTitle.replace(/ \\(.*\\)$/, "");
                    var currentEmail = (folderTitle.match(/\\((.+)\\)$/) || [])[1] || "";
                    if (currentEmail === "no email") currentEmail = "";
                    var newEmail = prompt("Email address for notifications (" + userId + "):", currentEmail);
                    if (newEmail !== null) {
                        clapshot.callOrganizer("set_user_email", {user_id: userId, email: newEmail.trim()});
                    }
                """).strip()))

    def make_cleanup_empty_user_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=clap.ActionUiProps(
                label="Del user",
                icon=clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-user-minus", color=clap.Color(r=220, g=38, b=38))),
                key_shortcut=None,
                natural_desc="Delete user if they have no content (only empty root folder)"),
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var folder = _action_args.selected_items?.[0]?.folder;
                    var folderId = folder?.id || null;
                    if (!folderId) {
                        alert("No user folder selected for cleanup");
                        return;
                    }
                    if (confirm("This will delete the user if they have no media files and only an empty root folder.\\n\\nComments from this user will be preserved but marked as from a deleted user.\\n\\nAre you sure?")) {
                        clapshot.callOrganizer("cleanup_empty_user", {folder_id: folderId});
                    }
                """).strip()))
