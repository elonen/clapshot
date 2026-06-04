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

from .folders import FoldersHelper
from organizer.database.models import DbMediaFile, DbFolder, DbUser, DbSetting


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
            await self._admin_show_smtp_config(ses, pg_items)

        page_id = folder_path_to_uri_arg([f.id for f in folder_path])
        return org.ClientShowPageRequest(sid=ses.sid, page_items=pg_items, page_id=page_id, page_title=cur_folder.title)


    async def _admin_show_all_user_homes(self, ses: org.UserSessionData, cur_folder: DbFolder, pg_items: list[clap.PageItem]):
        """
        For each user in the database, show a virtual folder that opens their home folder.
        Admin can also trash all user's content from here.
        """
        pg_items.append(clap.PageItem(html="<h3><strong>ADMIN</strong> – Dossiers utilisateurs</h3>"))

        pg_items.append(clap.PageItem(html="<p>Les utilisateurs suivants ont un dossier personnel et/ou des fichiers médias.<br/>Téléverser des fichiers ou déplacer des éléments dans ces dossiers transfèrera la propriété à cet utilisateur.<br/>Supprimer le dossier personnel d'un utilisateur effacera tout son contenu.</p>"))

        with self.db_new_session() as dbs:
            all_users: list[DbUser] = dbs.query(DbUser).order_by(DbUser.id).distinct().all()

        folders = []
        with self.db_new_session() as dbs:
            for user in all_users:

                if user.id == ses.user.id:
                    continue    # skip self, the view should already show user's own root folder

                users_folder = await db_get_or_create_user_root_folder(dbs, clap.UserInfo(id=user.id, name=user.name), self.srv, self.log)
                assert users_folder, f"User {user.id} has no root folder (should've been autocreated)"

                email_hint = f" ({user.email})" if user.email else " (sans email)"
                set_email_js = (
                    f"var e = prompt('Adresse email pour {html_escape(user.id)} :', {json.dumps(user.email or '')});"
                    f"if (e !== null) clapshot.callOrganizer('set_user_email', {{user_id: {json.dumps(user.id)}, email: e}});"
                )

                folders.append(
                    clap.PageItemFolderListingItem(
                        folder = clap.PageItemFolderListingFolder(
                            id = str(users_folder.id),
                            title = user.id + email_hint,
                            preview_items = []),
                        vis = clap.PageItemFolderListingItemVisualization(
                            icon = clap.Icon(
                                fa_class = clap.IconFaClass(classes="fas fa-user", color=clap.Color(r=184, g=160, b=148)),
                                size = 3.0),
                            base_color = clap.Color(r=160, g=100, b=50)),
                        popup_actions = ["popup_builtin_trash", "cleanup_empty_user", "set_user_email"],
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
            pg_items.append(clap.PageItem(html="""
                <div style="margin-top: 2em;">
                    <button onclick="if(confirm('Ceci supprimera TOUS les utilisateurs sans fichiers médias et avec uniquement un dossier personnel vide.\\n\\nLes commentaires des utilisateurs supprimés seront conservés mais marqués comme provenant d\\'utilisateurs supprimés.\\n\\nÊtes-vous sûr ?')) { clapshot.callOrganizer('cleanup_empty_user', {folder_id: '*'}); }"
                            style="background-color: #7f4f26; color: white; border: none; padding: 10px 20px; border-radius: 5px; cursor: pointer; font-weight: bold;">
                        🗑️ Supprimer les utilisateurs sans médias
                    </button>
                </div>
            """))


    async def _admin_show_smtp_config(self, ses: org.UserSessionData, pg_items: list[clap.PageItem]):
        """Affiche et permet de modifier la configuration SMTP dans l'admin."""
        with self.db_new_session() as dbs:
            rows = dbs.query(DbSetting).filter(
                DbSetting.key.in_(["smtp_host", "smtp_port", "smtp_user", "smtp_password", "smtp_from"])
            ).all()
            cfg = {r.key: r.value for r in rows}

        host     = cfg.get("smtp_host", "")
        port     = cfg.get("smtp_port", "587")
        user     = cfg.get("smtp_user", "")
        password = cfg.get("smtp_password", "")
        frm      = cfg.get("smtp_from", "")

        # Masquer le mot de passe
        password_display = "••••••••" if password else "(non défini)"
        status_color = "#22c55e" if host else "#ef4444"
        status_text  = "Configuré" if host else "Non configuré"

        html = f"""
        <div style="margin-top:2em;border:1px solid #374151;border-radius:8px;overflow:hidden;">
          <div style="background:#917a49;padding:14px 20px;display:flex;align-items:center;justify-content:space-between;">
            <h3 style="margin:0;color:#fff;font-size:16px;">✉️ Configuration Mail (SMTP)</h3>
            <span style="background:{status_color};color:#fff;padding:3px 10px;border-radius:12px;font-size:12px;font-weight:bold;">{status_text}</span>
          </div>
          <div style="padding:16px 20px;background:#1f2937;">
            <table style="width:100%;border-collapse:collapse;color:#d1d5db;font-size:14px;">
              <tr><td style="padding:6px 12px 6px 0;color:#9ca3af;width:140px;">Serveur SMTP</td><td style="padding:6px 0;"><strong>{html_escape(host) or '<em style="color:#6b7280">non défini</em>'}</strong></td></tr>
              <tr><td style="padding:6px 12px 6px 0;color:#9ca3af;">Port</td><td style="padding:6px 0;"><strong>{html_escape(port)}</strong></td></tr>
              <tr><td style="padding:6px 12px 6px 0;color:#9ca3af;">Utilisateur</td><td style="padding:6px 0;"><strong>{html_escape(user) or '<em style="color:#6b7280">non défini</em>'}</strong></td></tr>
              <tr><td style="padding:6px 12px 6px 0;color:#9ca3af;">Mot de passe</td><td style="padding:6px 0;"><strong>{password_display}</strong></td></tr>
              <tr><td style="padding:6px 12px 6px 0;color:#9ca3af;">Expéditeur</td><td style="padding:6px 0;"><strong>{html_escape(frm) or '<em style="color:#6b7280">non défini</em>'}</strong></td></tr>
            </table>
            <div style="margin-top:14px;display:flex;gap:10px;">
              <button onclick="(function(){{
                var host = prompt('Serveur SMTP (ex: mail.example.com):', {json.dumps(host)});
                if (host === null) return;
                var port = prompt('Port SMTP (465 = SSL, 587 = STARTTLS, 1025 = local):', {json.dumps(port)});
                if (port === null) return;
                var user = prompt('Utilisateur SMTP:', {json.dumps(user)});
                if (user === null) return;
                var pwd = prompt('Mot de passe SMTP (laisser vide pour ne pas changer):', '');
                if (pwd === null) return;
                var from_addr = prompt('Adresse expéditeur (From):', {json.dumps(frm)});
                if (from_addr === null) return;
                clapshot.callOrganizer('set_smtp_config', {{
                  host: host, port: port, user: user, password: pwd, from: from_addr
                }});
              }})()"
                style="background:#917a49;color:#fff;border:none;padding:8px 18px;border-radius:5px;cursor:pointer;font-weight:bold;">
                ✏️ Modifier
              </button>
              <button onclick="if(confirm('Effacer toute la configuration SMTP ?')) {{ clapshot.callOrganizer('set_smtp_config', {{host:'',port:'587',user:'',password:'',from:''}}); }}"
                style="background:#374151;color:#d1d5db;border:none;padding:8px 18px;border-radius:5px;cursor:pointer;">
                🗑️ Effacer
              </button>
            </div>
          </div>
        </div>
        """
        pg_items.append(clap.PageItem(html=html))


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

        async def media_file_to_page_item(vid_id: str, popup_actions: list[str]) -> clap.PageItemFolderListingItem:
            assert re.match(r"^[0-9a-fA-F]+$", vid_id), f"Unexpected media file ID format: {vid_id}"
            return clap.PageItemFolderListingItem(
                media_file = media_by_id[vid_id],
                open_action = clap.ScriptCall(
                    lang = clap.ScriptCallLang.JAVASCRIPT,
                    code = f'clapshot.openMediaFile("{vid_id}");'),
                popup_actions = popup_actions,
                vis = media_type_to_vis_icon(media_by_id[vid_id].media_type))

        listing_items: list[clap.PageItemFolderListingItem] = []
        for itm in folder_db_items:
            if isinstance(itm, DbFolder):
                listing_items.append(await self.folders_helper.folder_to_page_item(itm, popup_actions, ses))
            elif isinstance(itm, DbMediaFile):
                listing_items.append(await media_file_to_page_item(itm.id, popup_actions))
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
            popup_actions = ["new_folder"],
            listing_data = listing_data,
            allow_upload = can_upload,
            media_file_added_action = "on_media_file_added")

        pg_items = []
        pg_items.append(clap.PageItem(folder_listing=folder_listing))
        if len(folder_listing.items) == 0:
            if can_upload:
                pg_items.append(clap.PageItem(html="<p style='margin-top: 1em;'><i class='far fa-circle-question text-blue-400'></i> Use the drop zone to <strong>upload media files</strong>, or right-click on the empty space above to <strong>create a folder</strong>.</p>"))
                pg_items.append(clap.PageItem(html="<p>After that, drag items to <strong>reorder</strong>, or drop them <strong>into folders</strong>. Hold shift to multi-select.</p>"))

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
        return "<h3>Root folder</h3>"   # Fallback, should not happen in normal operation

    def _get_folder_display_title(folder: DbFolder, cur_user_id: str) -> str:
        # Indicate shared folders with user ID in brackets
        if folder.user_id != cur_user_id:
            return f"🔗 {folder.title} [{folder.user_id}]"

        # If it's the user's root folder, show "Home"
        if folder.id == user_root_folder_id:
            return "Home"

        return folder.title or "UNNAMED"


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
        _, current_title = breadcrumb_items[-1]
        breadcrumb_links.append(f"<strong>{html_escape(current_title)}</strong>")

    return f"<h3>{' ▶ '.join(breadcrumb_links)}</h3>"
