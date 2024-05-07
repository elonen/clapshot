import json
from typing import List, Optional

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import sqlalchemy
from sqlalchemy.orm import Session

import clapshot_grpc.clapshot as clap
import clapshot_grpc.clapshot.organizer as org

from organizer.database.models import DbFolder, DbFolderItems, DbVideo
from organizer.database.operations import db_get_or_create_user_root_folder
from organizer.utils import is_admin


class FoldersHelper:
    def __init__(self, db_new_session: sqlalchemy.orm.sessionmaker, srv: org.OrganizerOutboundStub, log):
        self.DbNewSession = db_new_session
        self.srv = srv
        self.log = log


    async def get_current_folder_path(self, ses: org.UserSessionData, cookie_override: Optional[str]) -> List[DbFolder]:
        """
        Get current folder path from cookies & DB.

        If the cookie is malformed, it will be replaced with an empty one.
        Returned list will always contain at least one item (root folder).

        If cookie_override is set, it will be used instead of the cookie from session.
        """
        res: List[DbFolder] = []
        ck = ses.cookies or {}
        with self.DbNewSession() as dbs:
            try:
                if folder_ids := json.loads((cookie_override or ck.get("folder_path")) or '[]'):
                    assert all(isinstance(i, int) for i in folder_ids), "Folder path cookie contains non-integer IDs"

                    folders_unordered = dbs.query(DbFolder).filter(DbFolder.id.in_(folder_ids)).all()
                    if len(folders_unordered) == len(folder_ids):
                        # Reorder the folders to match the order in the cookie, and return
                        folders_by_id = {f.id: f for f in folders_unordered}
                        return [folders_by_id[id] for id in folder_ids if id in folders_by_id]
                    else:
                        self.log.warning("Some unknown folder IDs in folder_path cookie. Clearing it.")
                        await self.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies={"folder_path": ''}, sid=ses.sid))
                        await self.srv.client_show_user_message(org.ClientShowUserMessageRequest(
                            sid=ses.sid,
                            msg=clap.UserMessage(
                                message="Some unknown folder IDs in folder_path cookie. Clearing it.",
                                user_id=ses.user.id,
                                type=clap.UserMessageType.ERROR)))

                self.log.debug("No folder_path cookie found. Returning empty folder path.")
                res = []
            except json.JSONDecodeError as e:
                self.log.error(f"Failed to parse folder_path cookie: {e}. Falling back to empty folder path.")
                res = []

            # Make sure root folder is always in the path
            user_root = await db_get_or_create_user_root_folder(dbs, ses, self.srv, self.log)
            if not res or res[0].id != user_root.id:
                res.insert(0, user_root)

        return res


    async def fetch_folder_contents(self, folder: DbFolder, user_id: str) -> List[DbVideo | DbFolder]:
        """
        Fetch the contents of a folder from the database, sorted by the order in the folder.
        """
        if folder.user_id != user_id and not is_admin(user_id):
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot fetch contents of another user's folder")

        with self.DbNewSession() as dbs:
            folder_items = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder.id).order_by(DbFolderItems.sort_order).all()

            # Get DbFolder and DbVideo objects for all folder items
            subfolder_ids = [fi.subfolder_id for fi in folder_items]
            subfolder_items = dbs.query(DbFolder).filter(DbFolder.id.in_(subfolder_ids)).all()
            subfolders_by_id = {f.id: f for f in subfolder_items}

            video_ids = [fi.video_id for fi in folder_items]
            video_items = dbs.query(DbVideo).filter(DbVideo.id.in_(video_ids)).all()
            videos_by_id = {v.id: v for v in video_items}

            # Replace folder item IDs with actual objects
            def _get_item(fi: DbFolderItems) -> DbVideo | DbFolder:
                if fi.video_id:
                    return videos_by_id[fi.video_id]
                elif fi.subfolder_id:
                    return subfolders_by_id[fi.subfolder_id]
                else:
                    raise ValueError("Folder item has neither video nor subfolder ID")
            res = [_get_item(fi) for fi in folder_items]

            # If all items have sort_order 0 (default) sort them by type and title
            if all(fi.sort_order == 0 for fi in folder_items):
                sorted_folders = sorted(subfolder_items, key=lambda f: f.title or str(f.id))
                sorted_videos = sorted(video_items, key=lambda v: v.title or str(v.id))
                res = sorted_folders + sorted_videos

            return res


    async def trash_folder_recursive(self, dbs: Session, folder_id: int, user_id: str) -> List[str]:
        """
        Trash a folder and unbind its contents recursively.
        Returns a list of all video IDs that are to be deleted.
        """
        fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
        if not fld:
            raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{folder_id}' not found")
        if fld.user_id != user_id and not is_admin(user_id):
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot trash another user's folder")

        folder_items = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder_id).all()
        video_ids = [it.video_id for it in folder_items if it.video_id]

        self.log.debug(f"Deleting folder '{folder_id}' ('{fld.title}') and its contents")

        # Recurse to subfolders
        for fi in [it.subfolder_id for it in folder_items if it.subfolder_id]:
            video_ids.extend(await self.trash_folder_recursive(dbs, fi, user_id))

        # Remove content bindings
        dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder_id).delete()

        # Delete the folder itself
        dbs.query(DbFolder).filter(DbFolder.id == folder_id).delete()
        return video_ids


    async def folder_to_page_item(self, fld: DbFolder, popup_actions: List[str], user_id: str) -> clap.PageItemFolderListingItem:
        """
        Convert a folder node to a page item.
        """
        pv_items = await self.preview_items_for_folder(fld, user_id)

        return clap.PageItemFolderListingItem(
            folder=clap.PageItemFolderListingFolder(
                id=str(fld.id),
                title=fld.title or "<UNNAMED>",
                preview_items=pv_items),
            open_action=clap.ScriptCall(
                lang=clap.ScriptCallLang.JAVASCRIPT,
                code=f'clapshot.callOrganizer("open_folder", {{id: {fld.id}}});'),
            popup_actions=popup_actions)


    async def preview_items_for_folder(self, fld: DbFolder, user_id: str) -> List[clap.PageItemFolderListingItem]:
        """
        Get preview items for a folder.
        Used in folder listings to show a preview of the folder contents (contained videos and subfolders).
        """
        contained_items = await self.fetch_folder_contents(fld, user_id)

        contained_videos = [itm for itm in contained_items if isinstance(itm, DbVideo)][:4]  # Client UI currently only shows max 4 items, don't bother with more
        video_objs: org.DbVideoList = await self.srv.db_get_videos(org.DbGetVideosRequest(ids=org.IdList(ids=[v.id for v in contained_videos])))
        videos_by_id = {v.id: v for v in video_objs.items}

        res = []
        for itm in contained_items:
            if isinstance(itm, DbFolder):
                res.append(clap.PageItemFolderListingItem(
                    folder=clap.PageItemFolderListingFolder(id=str(itm.id), title=itm.title or "<UNNAMED>")))
            elif isinstance(itm, DbVideo):
                res.append(clap.PageItemFolderListingItem(video=videos_by_id[itm.id]))
            else:
                raise ValueError(f"Unknown item type: {itm}")
        return res


    async def create_folder(self, dbs: Session, ses: org.UserSessionData, parent_folder: DbFolder, new_folder_name: str) -> DbFolder:
        """
        Create a new folder in the parent folder.
        """
        assert parent_folder is not None, "Cannot create root folders with this function"
        if parent_folder.user_id != ses.user.id and not is_admin(ses.user.id):
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot create folder in another user's folder")
        if len(new_folder_name) > 255:
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Folder name too long")
        if not new_folder_name:
            GRPCError(GrpcStatus.INVALID_ARGUMENT, "Folder name cannot be empty")

        if new_folder_name in [f.title for f in await self.fetch_folder_contents(parent_folder, ses.user.id)]:
            raise GRPCError(GrpcStatus.ALREADY_EXISTS, "Item with this name already exists in the this folder")

        with dbs.begin_nested():
            # Create the new folder
            new_folder = DbFolder(user_id=parent_folder.user_id, title=new_folder_name)
            dbs.add(new_folder)
            dbs.flush()

            # Add it at the end of the parent folder
            max_sort_order = dbs.query(sqlalchemy.func.max(DbFolderItems.sort_order)).filter(DbFolderItems.folder_id == parent_folder.id).scalar() or 0
            dbs.add(DbFolderItems(folder_id=parent_folder.id, subfolder_id=new_folder.id, sort_order=max_sort_order+1))
            return new_folder
