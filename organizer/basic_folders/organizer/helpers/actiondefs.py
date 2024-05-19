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
            "on_video_added": self.make_on_video_added_action(),
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

    def make_on_video_added_action(self) -> clap.ActionDef:
        return clap.ActionDef(
            ui_props=None,  # not an UI action, just a callback
            action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=dedent("""
                    var vid = _action_args.video_id;
                    var listingData = _action_args.listing_data;
                    var folderId = listingData?.folder_id;

                    if (!folderId || !vid) {
                        var msg = "on_video_added error: video_id missing, or folder_id from listingData.";
                        alert(msg); console.error(msg);
                    } else {
                        clapshot.moveToFolder(folderId, [{videoId: vid}], listingData);
                    }
                """).strip()))
