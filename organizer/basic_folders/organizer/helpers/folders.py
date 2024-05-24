from datetime import datetime
import json
from typing import List, Optional, Tuple

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import sqlalchemy
from sqlalchemy.orm import Session

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org
from clapshot_grpc.utilities import try_send_user_message

from organizer.config import PATH_COOKIE_NAME
from organizer.database.models import DbFolder, DbFolderItems, DbMediaFile
from organizer.database.operations import db_get_or_create_user_root_folder
from organizer.helpers import media_type_to_vis_icon


class FoldersHelper:
    def __init__(self, db_new_session: sqlalchemy.orm.sessionmaker, srv: org.OrganizerOutboundStub, log):
        self.db_new_session = db_new_session
        self.srv = srv
        self.log = log

    async def get_current_folder_path(self, ses: org.UserSessionData, cookie_override: Optional[str]=None) -> List[DbFolder]:
        """
        Get current folder path from cookies & DB.

        If the cookie is malformed, it will be replaced with an empty one.
        Returned list will always contain at least one item (root folder).

        If cookie_override is set, it will be used instead of the cookie from session.
        """
        res: List[DbFolder] = []
        ck = ses.cookies or {}
        with self.db_new_session() as dbs:
            try:
                if folder_ids := json.loads((cookie_override or ck.get(PATH_COOKIE_NAME)) or '[]'):
                    assert all(isinstance(i, int) for i in folder_ids), "Folder path cookie contains non-integer IDs"

                    folders_unordered = dbs.query(DbFolder).filter(DbFolder.id.in_(folder_ids)).all()
                    if len(folders_unordered) == len(folder_ids):
                        # Reorder the folders to match the order in the cookie, and return
                        folders_by_id = {f.id: f for f in folders_unordered}
                        return [folders_by_id[id] for id in folder_ids if id in folders_by_id]
                    else:
                        self.log.warning("Some unknown folder IDs in folder_path cookie. Clearing it.")
                        await self.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies={PATH_COOKIE_NAME: ''}, sid=ses.sid))
                        if err := await try_send_user_message(self.srv,
                                org.ClientShowUserMessageRequest(
                                    sid=ses.sid,
                                    msg=clap.UserMessage(
                                        message="Some unknown folder IDs in folder_path cookie. Clearing it.",
                                        user_id=ses.user.id,
                                        type=clap.UserMessageType.ERROR))):
                            self.log.error(f"Error calling client_show_user_message(): {err}")

                self.log.debug("No folder_path cookie found. Returning empty folder path.")
                res = []
            except json.JSONDecodeError as e:
                self.log.error(f"Failed to parse folder_path cookie: {e}. Falling back to empty folder path.")
                res = []

            # Make sure root folder is always in the path
            user_root = await db_get_or_create_user_root_folder(dbs, ses.user, self.srv, self.log)
            if not res or res[0].id != user_root.id:
                res.insert(0, user_root)

        return res

    async def fetch_folder_contents(self, folder: DbFolder, ses: org.UserSessionData) -> List[DbMediaFile | DbFolder]:
        """
        Fetch the contents of a folder from the database, sorted by the specified criteria.
        """
        if folder.user_id != ses.user.id and not ses.is_admin:
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot fetch contents of another user's folder")

        with self.db_new_session() as dbs:
            folder_items = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder.id).all()

            # Get DbFolder and DbMediaFile objects for all folder items
            subfolder_ids = [fi.subfolder_id for fi in folder_items if fi.subfolder_id]
            subfolder_items = dbs.query(DbFolder).filter(DbFolder.id.in_(subfolder_ids)).all()
            subfolders_by_id = {f.id: f for f in subfolder_items}

            media_ids = [fi.media_file_id for fi in folder_items if fi.media_file_id]
            media_items = dbs.query(DbMediaFile).filter(DbMediaFile.id.in_(media_ids)).all()
            media_by_id = {v.id: v for v in media_items}

            # Replace folder item IDs with actual objects and their sort_order
            def _get_item(fi: DbFolderItems) -> Tuple[int, DbMediaFile | DbFolder]:
                if fi.media_file_id:
                    return (fi.sort_order, media_by_id[fi.media_file_id])
                elif fi.subfolder_id:
                    return (fi.sort_order, subfolders_by_id[fi.subfolder_id])
                else:
                    raise ValueError("Folder item has neither media file nor subfolder ID")

            items_with_sort_order = [_get_item(fi) for fi in folder_items]

            # Sort by sort_order first, then by type, and then by .created or .added_time (newest first)
            sorted_items = sorted(items_with_sort_order, key=lambda x: (
                x[0],
                isinstance(x[1], DbMediaFile),
                -(getattr(x[1], 'added_time', getattr(x[1], 'created', datetime(1970, 1, 1))).timestamp())
            ))

            # Extract the sorted objects
            res = [item[1] for item in sorted_items]

            return res

    async def trash_folder_recursive(self, dbs: Session, folder_id: int, ses: org.UserSessionData) -> List[str]:
        """
        Trash a folder and unbind its contents recursively.
        Returns a list of all media file IDs that are to be deleted.
        """
        fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
        if not fld:
            raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{folder_id}' not found")
        if fld.user_id != ses.user.id and not ses.is_admin:
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot trash another user's folder")

        folder_items = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder_id).all()
        media_ids = [it.media_file_id for it in folder_items if it.media_file_id]

        self.log.debug(f"Deleting folder '{folder_id}' ('{fld.title}') and its contents")

        # Recurse to subfolders
        for fi in [it.subfolder_id for it in folder_items if it.subfolder_id]:
            media_ids.extend(await self.trash_folder_recursive(dbs, fi, ses))

        # Remove content bindings
        dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder_id).delete()

        # Delete the folder itself
        dbs.query(DbFolder).filter(DbFolder.id == folder_id).delete()
        return media_ids

    async def folder_to_page_item(self, fld: DbFolder, popup_actions: List[str], ses: org.UserSessionData) -> clap.PageItemFolderListingItem:
        """
        Convert a folder node to a page item.
        """
        pv_items = await self.preview_items_for_folder(fld, ses)

        return clap.PageItemFolderListingItem(
            folder = clap.PageItemFolderListingFolder(
                id = str(fld.id),
                title = fld.title or "<UNNAMED>",
                preview_items = pv_items),
            open_action = clap.ScriptCall(
                lang = clap.ScriptCallLang.JAVASCRIPT,
                code = f'clapshot.callOrganizer("open_folder", {{id: {fld.id}}});'),
            popup_actions = popup_actions)

    async def preview_items_for_folder(self, fld: DbFolder, ses: org.UserSessionData) -> List[clap.PageItemFolderListingItem]:
        """
        Get preview items for a folder.
        Used in folder listings to show a preview of the folder contents (contained media files and subfolders).
        """
        contained_items = await self.fetch_folder_contents(fld, ses)

        media_files = [item for item in contained_items if isinstance(item, DbMediaFile)][:4]
        folders = [item for item in contained_items if isinstance(item, DbFolder)][:4]

        if media_files:
            media_details = await self.srv.db_get_media_files(org.DbGetMediaFilesRequest(ids=org.IdList(ids=[v.id for v in media_files])))
            media_by_id = {v.id: v for v in media_details.items}

        # Prepare result list with up to 4 items, prioritizing media files
        result = [
            clap.PageItemFolderListingItem(
                media_file=media_by_id[v.id],
                vis=media_type_to_vis_icon(media_by_id[v.id].media_type))
            for v in media_files
        ] + [
            clap.PageItemFolderListingItem(
                folder=clap.PageItemFolderListingFolder(
                    id=str(f.id), title=f.title or "???")
            )
            for f in folders[: 4 - len(media_files)]
        ]

        return result

    async def create_folder(self, dbs: Session, ses: org.UserSessionData, parent_folder: DbFolder, new_folder_name: str) -> DbFolder:
        """
        Create a new folder in the parent folder.
        """
        assert parent_folder is not None, "Cannot create root folders with this function"
        if parent_folder.user_id != ses.user.id and not ses.is_admin:
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot create folder in another user's folder")
        if len(new_folder_name) > 255:
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Folder name too long")
        if not new_folder_name:
            GRPCError(GrpcStatus.INVALID_ARGUMENT, "Folder name cannot be empty")

        if new_folder_name in [f.title for f in await self.fetch_folder_contents(parent_folder, ses)]:
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
