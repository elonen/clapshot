import re
from typing import Optional
import json
from html import escape as html_escape

import clapshot_grpc.clapshot as clap
import clapshot_grpc.clapshot.organizer as org

from .folders import FoldersHelper
from organizer.database.models import DbVideo, DbFolder


class PagesHelper:
    def __init__(self, folders_helper: FoldersHelper, srv: org.OrganizerOutboundStub):
        self.folders_helper = folders_helper
        self.srv = srv


    async def construct_navi_page(self, ses: org.UserSessionData, cookie_override: Optional[str] = None) -> org.ClientShowPageRequest:
        """
        Construct the main navigation page for given user session.
        """
        folder_path = await self.folders_helper.get_current_folder_path(ses, cookie_override)
        assert len(folder_path) > 0, "Folder path should always contain at least the root folder"
        cur_folder = folder_path[-1]

        folder_db_items = await self.folders_helper.fetch_folder_contents(cur_folder, ses.user.id)

        popup_actions = ["popup_builtin_rename", "popup_builtin_trash"]
        listing_data = {"folder_id": str(cur_folder.id)}

        if len(folder_path) > 1:
            # If not in root folder, add "move to parent" action to all items
            popup_actions.append("move_to_parent")
            listing_data["parent_folder_id"] = str(folder_path[-2].id)

        video_ids = [v.id for v in folder_db_items if isinstance(v, DbVideo)]
        video_list = await self.srv.db_get_videos(org.DbGetVideosRequest(ids=org.IdList(ids=video_ids)))
        videos_by_id = {v.id: v for v in video_list.items}

        async def video_to_page_item(vid_id: str, popup_actions: list[str]) -> clap.PageItemFolderListingItem:
            assert re.match(r"^[0-9a-fA-F]+$", vid_id), f"Unexpected video ID format: {vid_id}"
            return clap.PageItemFolderListingItem(
                video=videos_by_id[vid_id],
                open_action=clap.ScriptCall(
                    lang=clap.ScriptCallLang.JAVASCRIPT,
                    code=f'clapshot.openVideo("{vid_id}");'),
                popup_actions=popup_actions,
                vis=None)

        listing_items: list[clap.PageItemFolderListingItem] = []
        for itm in folder_db_items:
            if isinstance(itm, DbFolder):
                listing_items.append(await self.folders_helper.folder_to_page_item(itm, popup_actions, ses.user.id))
            elif isinstance(itm, DbVideo):
                listing_items.append(await video_to_page_item(itm.id, popup_actions))
            else:
                raise ValueError(f"Unknown item type: {itm}")

        folder_listing = clap.PageItemFolderListing(
            items=listing_items,
            allow_reordering=True,
            popup_actions=["new_folder"],
            listing_data=listing_data,
            allow_upload=True,
            video_added_action="on_video_added")

        def make_breadcrumbs_html(folder_path: list[DbFolder]) -> Optional[str]:
            if not folder_path:
                return None

            breadcrumbs: list[tuple[int, str]] = [(f.id, str(f.title or "UNNAMED")) for f in folder_path]
            breadcrumbs[0] = (breadcrumbs[0][0], "[Home]")  # rename root folder to "Home" for UI
            breadcrumbs_html = []

            # Link all but last item
            for (id, title) in breadcrumbs[:-1]:
                args_json = json.dumps({'id': id}).replace('"', "'")
                title = html_escape(title)
                breadcrumbs_html.append(
                    f'<a style="text-decoration: underline;" href="javascript:clapshot.callOrganizer(\'open_folder\', {args_json});">{title}</a>')
            # Last item in bold
            for (_, title) in breadcrumbs[-1:]:
                breadcrumbs_html.append(f"<strong>{html_escape(title)}</strong>")

            return " ▶ ".join(breadcrumbs_html) if len(breadcrumbs) > 1 else None

        pg_items = []

        if html := make_breadcrumbs_html(folder_path):
            pg_items.append(clap.PageItem(html=html))  # add to first pos

        if len(folder_listing.items) == 0:
            pg_items.append(clap.PageItem(html="<h2>Folder is empty.</h2>"))

        pg_items.append(clap.PageItem(folder_listing=folder_listing))

        return org.ClientShowPageRequest(sid=ses.sid, page_items=pg_items)
