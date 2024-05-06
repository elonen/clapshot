import json
import re
from typing import Optional
from logging import Logger

import grpclib
from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus
from grpclib.server import Server

import clapshot_grpc.clapshot as clap
import clapshot_grpc.clapshot.organizer as org

import sqlalchemy
from sqlalchemy.orm import sessionmaker, Session

from config import VERSION, MODULE_NAME, PATH_COOKIE_NAME
from organizer.utils import is_admin

from .database.operations import db_check_and_fix_integrity, db_check_pending_migrations, db_apply_migration, db_test_orm_mappings
from .database.models import DbFolder, DbFolderItems, DbVideo


async def move_to_folder(oi, req: org.MoveToFolderRequest) -> clap.Empty:
    """
    Organizer method (gRPC/protobuf)

    Called when user moves a list of items (folders or videos) to a new parent folder in the client UI.
        => Add (or update) the folder_id field in the DbFolderItems table for each item in req.ids.
    """
    if not req.ids:
        oi.log.warning("move_to_folder called with empty list of items. Bug in client?")
        return clap.Empty()

    with oi.DbNewSession() as dbs:
        with dbs.begin_nested():
            dst_folder = dbs.query(DbFolder).filter(DbFolder.id == int(req.dst_folder_id)).one_or_none()
            max_sort_order = dbs.query(sqlalchemy.func.max(DbFolderItems.sort_order)).filter(DbFolderItems.folder_id == int(req.dst_folder_id)).scalar() or 0

            if not dst_folder:
                raise GRPCError(GrpcStatus.NOT_FOUND, "Destination folder not found")
            if dst_folder.user_id != req.ses.user.id and not is_admin(req.ses.user.id):
                raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot move items to another user's folder")

            for it in req.ids:
                # Move a folder
                if it.folder_id:
                    fld_to_move = dbs.query(DbFolder).filter(DbFolder.id == int(it.folder_id)).one_or_none()

                    if not fld_to_move:
                        raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder id '{it.folder_id}' not found")
                    if fld_to_move.id == dst_folder.id:
                        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Cannot move a folder into itself")
                    if fld_to_move.user_id != req.ses.user.id and not is_admin(req.ses.user.id):
                        raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot move another user's folder")

                    fld_to_move.user_id = dst_folder.user_id  # transfer ownership
                    cnt = dbs.query(DbFolderItems).filter(DbFolderItems.subfolder_id == fld_to_move.id).update({"folder_id": dst_folder.id, "sort_order": max_sort_order+1})
                    if cnt == 0:
                        raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder with ID '{fld_to_move.id}' is a root folder? Cannot move.")
                    else:
                        oi.log.debug(f"Moved folder '{fld_to_move.id}' to folder '{dst_folder.id}'")

                # Move a video
                elif it.video_id:
                    vid_to_move = dbs.query(DbVideo).filter(DbVideo.id == it.video_id).one_or_none()

                    if not vid_to_move:
                        raise GRPCError(GrpcStatus.NOT_FOUND, f"Video '{it.video_id}' not found")
                    if vid_to_move.user_id != req.ses.user.id and not is_admin(req.ses.user.id):
                        raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot move another user's video")

                    vid_to_move.user_id = dst_folder.user_id  # transfer ownership
                    cnt = dbs.query(DbFolderItems).filter(DbFolderItems.video_id == vid_to_move.id).update({"folder_id": dst_folder.id, "sort_order": max_sort_order+1})
                    if cnt == 0:  # not in any folder yet => insert it
                        dbs.add(DbFolderItems(folder_id=dst_folder.id, video_id=vid_to_move.id, sort_order=max_sort_order+1))
                    else:
                        oi.log.debug(f"Moved video '{vid_to_move.id}' to folder '{dst_folder.id}'")

    # Update page to view the opened folder (after transaction commit!)
    page = await oi.pages_helper.construct_navi_page(req.ses, None)
    await oi.srv.client_show_page(page)
    return clap.Empty()


async def reorder_items(oi, req: org.ReorderItemsRequest) -> clap.Empty:
    """
    Organizer (gRPC/protobuf)
    Called when user reorders items in a folder in the client UI.
      => Use the order of items in req.ids to update the sort_order field in the database.
    """
    if not req.ids:
        oi.log.warning("reorder_items called with empty list of items. Bug in client?")
        return clap.Empty()

    if parent_folder_id := req.listing_data.get("folder_id"):
        with oi.DbNewSession() as dbs:
            with dbs.begin_nested():

                # Check destination folder
                parent_folder = dbs.query(DbFolder).filter(DbFolder.id == int(parent_folder_id)).one_or_none()
                if not parent_folder:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Parent folder {parent_folder_id} not found")
                if parent_folder.user_id != req.ses.user.id and not is_admin(req.ses.user.id):
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot reorder items in another user's folder")

                # Reorder items
                for i, it in enumerate(req.ids):
                    if it.folder_id:
                        cnt = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == parent_folder.id, DbFolderItems.subfolder_id == int(it.folder_id)).update({"sort_order": i})
                        if cnt == 0:
                            oi.log.warning(f"DB inconsistency? Folder ID '{it.folder_id}' not in folder '{parent_folder.id}. Reordering skipped'")
                    elif it.video_id:
                        cnt = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == parent_folder.id, DbFolderItems.video_id == it.video_id).update({"sort_order": i})
                        if cnt == 0:
                            oi.log.warning(f"DB inconsistency? Video ID '{it.video_id}' not in folder '{parent_folder.id}. Reordering skipped'")

                return clap.Empty()
    else:
        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "No folder ID in UI listing, cannot reorder")
