import re
from typing import Optional
import json
from html import escape as html_escape

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org
from sqlalchemy import orm

from organizer.database.operations import db_get_or_create_user_root_folder
from organizer.helpers import media_type_to_vis_icon
from organizer.utils import folder_path_to_uri_arg
import organizer.metaplugin as mp

from .folders import FoldersHelper, version_number, VERSION_BADGE_COLOR, VERSION_ACTIVE_COLOR
from .l10n import _, pgettext
from organizer.database.models import DbMediaFile, DbFolder, DbUser


class PagesHelper:
    def __init__(self, folders_helper: FoldersHelper, srv: org.OrganizerOutboundStub, db_new_session: orm.sessionmaker, log, organizer_inbound=None):
        self.folders_helper = folders_helper
        self.srv = srv
        self.db_new_session = db_new_session
        self.log = log
        self.organizer_inbound = organizer_inbound


    async def construct_navi_page(self, ses: org.UserSessionData, cookie_override: Optional[str] = None) -> org.ClientShowPageRequest:
        """
        Construct the main navigation page for given user session.
        """

        folder_path, user_root_folder = await self.folders_helper.get_current_folder_path(ses, cookie_override)
        assert len(folder_path) > 0, "Folder path should always contain at least the root folder"

        cur_folder = folder_path[-1]
        parent_folder = folder_path[-2] if len(folder_path) > 1 else None

        if self.organizer_inbound:
            self.organizer_inbound.folder_viewer_tracker.register(cur_folder.id, ses.sid)

        pg_items: list[clap.PageItem] = []

        pg_items.append(clap.PageItem(html=_make_breadcrumbs_html(folder_path, ses.user.id, user_root_folder.id)))

        folder_db_items = await self.folders_helper.fetch_folder_contents(cur_folder, ses)
        pg_items.extend(await self._make_folder_listing(folder_db_items, cur_folder, parent_folder, ses))

        if ses.is_admin and len(folder_path) == 1:
            await self._admin_show_all_user_homes(ses, cur_folder, pg_items)

        page_id = folder_path_to_uri_arg([f.id for f in folder_path])
        return org.ClientShowPageRequest(sid=ses.sid, page_items=pg_items, page_id=page_id, page_title=cur_folder.title)


    async def _admin_show_all_user_homes(self, ses: org.UserSessionData, cur_folder: DbFolder, pg_items: list[clap.PageItem]):
        """
        For each user in the database, show a virtual folder that opens their home folder.
        Admin can also trash all user's content from here.
        """
        pg_items.append(clap.PageItem(html=_("<h3><strong>ADMIN</strong> – User Folders</h3>")))

        pg_items.append(clap.PageItem(html=_("<p>The following users currently have a home folder and/or media files.<br/>Uploading files or moving items to these folders will transfer ownership to that user.<br/>Trashing a user's home folder will delete everything they have.</p>")))

        with self.db_new_session() as dbs:
            all_users: list[DbUser] = dbs.query(DbUser).order_by(DbUser.id).distinct().all()

        folders = []
        with self.db_new_session() as dbs:
            for user in all_users:

                if user.id == ses.user.id:
                    continue    # skip self, the view should already show user's own root folder

                users_folder = await db_get_or_create_user_root_folder(dbs, clap.UserInfo(id=user.id, name=user.name), self.srv, self.log)
                assert users_folder, f"User {user.id} has no root folder (should've been autocreated)"

                folders.append(
                    clap.PageItemFolderListingItem(
                        folder = clap.PageItemFolderListingFolder(
                            id = str(users_folder.id),
                            title = user.id,
                            preview_items = []),
                        vis = clap.PageItemFolderListingItemVisualization(
                            icon = clap.Icon(
                                fa_class = clap.IconFaClass(classes="fas fa-user", color=clap.Color(r=184, g=160, b=148)),
                                size = 3.0),
                            base_color = clap.Color(r=160, g=100, b=50)),
                        popup_actions = ["popup_builtin_trash", "cleanup_empty_user"],
                        open_action = clap.ScriptCall(
                            lang = clap.ScriptCallLang.JAVASCRIPT,
                            code = f'clapshot.callOrganizer("open_folder", {{id: {users_folder.id}}});'),
                        ))

            user_folder_listing = clap.PageItemFolderListing(
                items = folders,
                popup_actions = [],  # don't allow any actions on this virtual view
                listing_data = {"folder_id": str(cur_folder.id)},
                allow_reordering = False,
                allow_upload = False,
                media_file_added_action = None)

            pg_items.append(clap.PageItem(folder_listing=user_folder_listing))

            # Add batch cleanup button
            confirm_msg = _("This will delete ALL users who have no media files and only empty root folders.\n\nComments from deleted users will be preserved but marked as from deleted users.\n\nAre you sure?")
            pg_items.append(clap.PageItem(html=f"""
                <div style="margin-top: 2em;">
                    <button onclick="if(confirm({json.dumps(confirm_msg)})) {{ clapshot.callOrganizer('cleanup_empty_user', {{folder_id: '*'}}); }}"
                            style="background-color: #7f4f26; color: white; border: none; padding: 10px 20px; border-radius: 5px; cursor: pointer; font-weight: bold;">
                        🗑️ {_("Delete all users without media")}
                    </button>
                </div>
            """))


    async def _make_folder_listing(
            self,
            folder_db_items: list[DbMediaFile | DbFolder],
            cur_folder: DbFolder,
            parent_folder: Optional[DbFolder],
            ses: org.UserSessionData) -> list[clap.PageItem]:
        """
        Make a folder listing for given folder and its contents.
        """
        popup_actions = ["popup_builtin_rename", "popup_builtin_trash"]
        listing_data = {"folder_id": str(cur_folder.id)}

        if parent_folder:
            # If not in root folder, add "move to parent" action to all items
            popup_actions.append("move_to_parent")
            listing_data["parent_folder_id"] = str(parent_folder.id)

        # Collect access tokens for all shared folders in this listing.
        # This is used to show shared folder links in the UI.
        folder_items = [item for item in folder_db_items if isinstance(item, DbFolder)]
        if shared_folder_tokens := self.folders_helper.get_shared_folder_tokens(folder_items):
            listing_data["shared_folder_tokens"] = json.dumps(shared_folder_tokens)

        # Fetch media files in this folder
        media_ids = [v.id for v in folder_db_items if isinstance(v, DbMediaFile)]
        media_list = await self.srv.db_get_media_files(org.DbGetMediaFilesRequest(ids=org.IdList(ids=media_ids)))
        media_by_id = {v.id: v for v in media_list.items}

        async def media_file_to_page_item(vid_id: str, popup_actions: list[str], vis: Optional[clap.PageItemFolderListingItemVisualization] = None) -> clap.PageItemFolderListingItem:
            assert re.match(r"^[0-9a-fA-F]+$", vid_id), f"Unexpected media file ID format: {vid_id}"
            return clap.PageItemFolderListingItem(
                media_file = media_by_id[vid_id],
                open_action = clap.ScriptCall(
                    lang = clap.ScriptCallLang.JAVASCRIPT,
                    code = f'clapshot.openMediaFile("{vid_id}");'),
                popup_actions = popup_actions,
                vis = vis if vis is not None else media_type_to_vis_icon(media_by_id[vid_id].media_type))

        # In a version set's "Manage Versions" view, each item is a version: orange vN badge,
        # the Active one tinted cyan, and a per-version "Set Active Ver" popup.
        # Read-only classification (the current set may itself be invalid -- render defensively as a
        # normal folder rather than mutating it here; repair_version_sets() does the actual fix-up).
        is_vset = self.folders_helper.renders_as_version_set(cur_folder, folder_db_items)
        ordered_media = [it for it in folder_db_items if isinstance(it, DbMediaFile)]
        vset_active_id = self.folders_helper.displayed_active_id(cur_folder, ordered_media) if is_vset else None
        vset_total = len(ordered_media)
        vset_left_index = {m.id: i for i, m in enumerate(ordered_media)}

        listing_items: list[clap.PageItemFolderListingItem] = []
        for itm in folder_db_items:
            if isinstance(itm, DbFolder):
                listing_items.append(await self.folders_helper.folder_to_page_item(itm, popup_actions, ses))
            elif isinstance(itm, DbMediaFile):
                if is_vset:
                    n = version_number(vset_left_index[itm.id], vset_total)
                    base_icon = media_type_to_vis_icon(media_by_id[itm.id].media_type)
                    vis = clap.PageItemFolderListingItemVisualization(
                        icon = base_icon.icon,
                        badges = [clap.PageItemFolderListingItemVisualizationBadge(text=f"v{n}", color=VERSION_BADGE_COLOR)],
                        base_color = VERSION_ACTIVE_COLOR if itm.id == vset_active_id else None)
                    listing_items.append(await media_file_to_page_item(itm.id, popup_actions + ["set_active_version"], vis=vis))
                else:
                    # Media files in a normal folder can be grouped into a new version set.
                    listing_items.append(await media_file_to_page_item(itm.id, popup_actions + ["make_versioned"]))
            else:
                raise ValueError(f"Unknown item type: {itm}")

        # Let metaplugins augment folder listing items and data
        if self.organizer_inbound:
            folder_context = mp.FolderContext(folder=cur_folder, parent=parent_folder)
            listing_items = await self.organizer_inbound.metaplugin_loader.call_augment_folder_listing_hooks(
                listing_items, folder_context, ses)
            listing_data = await self.organizer_inbound.metaplugin_loader.call_augment_listing_data_hooks(
                listing_data, folder_context, ses)

        # Only allow uploads if user owns the folder or is admin, AND has upload permission
        from ..authz_methods import check_upload_permission
        upload_permission = check_upload_permission(ses)
        # If no header found (None), default to allow for backward compatibility
        has_upload_permission = upload_permission is not False
        can_upload = (cur_folder.user_id == ses.user.id or ses.is_admin) and has_upload_permission

        folder_listing = clap.PageItemFolderListing(
            items = listing_items,
            allow_reordering = True,
            popup_actions = [] if is_vset else ["new_folder"],   # version sets can't hold subfolders
            listing_data = listing_data,
            allow_upload = can_upload,
            media_file_added_action = "on_media_file_added")

        pg_items = []
        pg_items.append(clap.PageItem(folder_listing=folder_listing))
        if len(folder_listing.items) == 0:
            if can_upload:
                pg_items.append(clap.PageItem(html=_("<p style='margin-top: 1em;'><i class='far fa-circle-question text-blue-400'></i> Use the drop zone to <strong>upload media files</strong>, or right-click on the empty space above to <strong>create a folder</strong>.</p>")))
                pg_items.append(clap.PageItem(html=_("<p>After that, drag items to <strong>reorder</strong>, or drop them <strong>into folders</strong>. Hold shift to multi-select.</p>")))

        return pg_items


def _make_breadcrumbs_html(folder_path: list[DbFolder], cur_user_id: str, user_root_folder_id: int) -> str:
    """
    Generate HTML breadcrumb navigation from folder path.

    Returns a styled breadcrumb trail with clickable links for all folders
    except the current one. Shows shared folder indicators and handles
    the home folder specially.

    Args:
        folder_path: List of folders from root to current folder
        cur_user_id: ID of current user to identify shared folders

    Returns:
        HTML string with breadcrumb navigation in <h3> tags
    """
    if not folder_path:
        return f"<h3>{_('Root folder')}</h3>"   # Fallback, should not happen in normal operation

    def _get_folder_display_title(folder: DbFolder, cur_user_id: str) -> str:
        # Indicate shared folders with user ID in brackets
        if folder.user_id != cur_user_id:
            return f"🔗 {folder.title} [{folder.user_id}]"

        # If it's the user's root folder, show "Home"
        if folder.id == user_root_folder_id:
            # TRANSLATORS: breadcrumb label for the user's own home folder (not "house")
            return pgettext("folder", "Home")

        return folder.title or pgettext("folder", "UNNAMED")


    def _create_folder_link(folder_id: int, title: str) -> str:
        # Create a clickable HTML link for a folder in breadcrumbs.
        escaped_title = html_escape(title)
        args_json = json.dumps({'id': folder_id}).replace('"', "'")
        return (
            f'<a style="text-decoration: underline;" '
            f'href="javascript:clapshot.callOrganizer(\'open_folder\', {args_json});">'
            f'{escaped_title}</a>'
        )


    # Build breadcrumb items with titles and shared folder detection
    breadcrumb_items = [(folder.id, _get_folder_display_title(folder, cur_user_id)) for folder in folder_path]

    # Generate HTML for breadcrumb trail
    breadcrumb_links = []

    # All items except last as clickable links
    for folder_id, title in breadcrumb_items[:-1]:
        link_html = _create_folder_link(folder_id, title)
        breadcrumb_links.append(link_html)

    # Last item in bold and non-clickable (current folder)
    if breadcrumb_items:
        _unused, current_title = breadcrumb_items[-1]
        breadcrumb_links.append(f"<strong>{html_escape(current_title)}</strong>")

    return f"<h3>{' ▶ '.join(breadcrumb_links)}</h3>"
