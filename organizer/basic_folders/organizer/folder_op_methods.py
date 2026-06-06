from __future__ import annotations
import json
from logging import Logger
from typing import Optional

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org

import sqlalchemy
from sqlalchemy import orm
from .database.models import DbFolder, DbFolderItems, DbMediaFile, DbUser

from organizer.config import PATH_COOKIE_NAME

import organizer

async def move_to_folder_impl(oi: organizer.OrganizerInbound, req: org.MoveToFolderRequest) -> clap.Empty:
    """
    Organizer method (gRPC/protobuf)

    Called when user moves a list of items (folders or media files) to a new parent folder in the client UI.
        => Add (or update) the folder_id field in the DbFolderItems table for each item in req.ids.
    """
    if not req.ids:
        oi.log.warning("move_to_folder called with empty list of items. Bug in client?")
        return clap.Empty()

    with oi.db_new_session() as dbs:
        dst_folder = dbs.query(DbFolder).filter(DbFolder.id == int(req.dst_folder_id)).one_or_none()
        min_sort_order = dbs.query(sqlalchemy.func.min(DbFolderItems.sort_order)).filter(DbFolderItems.folder_id == int(req.dst_folder_id)).scalar() or 0

    if not dst_folder:
        raise GRPCError(GrpcStatus.NOT_FOUND, "Destination folder not found")

    # Check authorization via metaplugins + default checks
    from .authz_methods import check_action_authorization
    await check_action_authorization(oi, "move_item", folder=dst_folder, ses=req.ses)

    # Version-set destination: media files only, and remember prior active/latest ver
    dst_is_vset = dst_folder.is_version_set
    prior_active: Optional[str] = None
    prior_latest_ver: Optional[str] = None
    if dst_is_vset:
        if any(it.folder_id for it in req.ids):
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "A version set can only contain media files")
        prior_media = [it for it in await oi.folders_helper.fetch_folder_contents(dst_folder, req.ses) if isinstance(it, DbMediaFile)]
        prior_latest_ver = prior_media[0].id if prior_media else None
        prior_active = dst_folder.active_media_file_id


    # Front-insert moved items before the current latest, preserving req.ids order (index 0 ends
    # up leftmost/latest). Distinct descending sort_orders keep multi-item moves deterministic -- a version
    # set's "new latest"/active then follows selection order rather than creation-time tiebreaks.
    for idx, it in enumerate(req.ids):
        item_sort_order = min_sort_order - len(req.ids) + idx
        with oi.db_new_session() as dbs:
            # Move a folder
            if it.folder_id:
                fld_to_move: Optional[DbFolder] = dbs.query(DbFolder).filter(DbFolder.id == int(it.folder_id)).one_or_none()

                if not fld_to_move:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder id '{it.folder_id}' not found")
                if fld_to_move.id == dst_folder.id:
                    raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Cannot move a folder into itself")
                if fld_to_move.user_id != req.ses.user.id and not req.ses.is_admin:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot move another user's folder")


                with dbs.begin_nested():
                    cnt = dbs.query(DbFolderItems).filter(DbFolderItems.subfolder_id == fld_to_move.id).update({"folder_id": dst_folder.id, "sort_order": item_sort_order})
                    if cnt == 0:
                        raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder with ID '{fld_to_move.id}' is a root folder? Cannot move.")
                    assert dst_folder.user_id, "Destination folder has no user ID, cannot transfer ownership"

                    await _recursive_set_folder_owner(dbs, fld_to_move.id, dst_folder.user_id, set(), oi.log)

                oi.log.debug(f"Moved folder '{fld_to_move.id}' to folder '{dst_folder.id}'")

            # Move a media file
            elif it.media_file_id:
                vid_to_move = dbs.query(DbMediaFile).filter(DbMediaFile.id == it.media_file_id).one_or_none()

                if not vid_to_move:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Media file '{it.media_file_id}' not found")
                if vid_to_move.user_id != req.ses.user.id and not req.ses.is_admin:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot move another user's media file")


                with dbs.begin_nested():
                    vid_to_move.user_id = dst_folder.user_id  # transfer ownership
                    cnt = dbs.query(DbFolderItems).filter(DbFolderItems.media_file_id == vid_to_move.id).update({"folder_id": dst_folder.id, "sort_order": item_sort_order})
                    if cnt == 0:  # not in any folder yet => insert it
                        dbs.add(DbFolderItems(folder_id=dst_folder.id, media_file_id=vid_to_move.id, sort_order=item_sort_order))
                    else:
                        oi.log.debug(f"Moved media file '{vid_to_move.id}' to folder '{dst_folder.id}'")


    # --- Version-set bookkeeping (after the moves) ---

    # Destination is a version set: the moved-in file is the new latest. The active
    # version follows the new latest only if the previous active was the previous latest.
    if dst_is_vset:
        new_media = [it for it in await oi.folders_helper.fetch_folder_contents(dst_folder, req.ses) if isinstance(it, DbMediaFile)]
        new_latest = new_media[0].id if new_media else None
        if new_latest and (prior_active is None or prior_active == prior_latest_ver):
            with oi.db_new_session() as dbs:
                with dbs.begin_nested():
                    dbs.query(DbFolder).filter(DbFolder.id == dst_folder.id).update({"active_media_file_id": new_latest})

    # Source is a version set we moved item(s) OUT of: fix its active pointer, or delete it if now empty.
    redirect_parent_id: Optional[int] = None
    src_folder_id_str = req.listing_data.get("folder_id")
    if src_folder_id_str and int(src_folder_id_str) != dst_folder.id:
        src_id = int(src_folder_id_str)
        with oi.db_new_session() as dbs:
            src = dbs.query(DbFolder).filter(DbFolder.id == src_id).one_or_none()
            # Only touch the source set if the acting user owns it (admins included). The moved item
            # was already ownership-checked above; this guards the source-folder mutation/deletion.
            if src and src.is_version_set and (src.user_id == req.ses.user.id or req.ses.is_admin):
                src_media = [it for it in await oi.folders_helper.fetch_folder_contents(src, req.ses) if isinstance(it, DbMediaFile)]
                if not src_media:
                    parent_link = dbs.query(DbFolderItems).filter(DbFolderItems.subfolder_id == src_id).first()
                    redirect_parent_id = parent_link.folder_id if parent_link else None
                    with dbs.begin_nested():
                        oi.folders_helper.delete_folder_and_links(dbs, src_id)
                    oi.log.info(f"Version set {src_id} emptied by move-out; deleted it.")
                elif src.active_media_file_id not in [m.id for m in src_media]:
                    with dbs.begin_nested():
                        src.active_media_file_id = src_media[0].id

    # Update page (redirect to the parent folder if the source version set was just deleted).
    if redirect_parent_id is not None:
        with oi.db_new_session() as dbs:
            ancestors = await oi.folders_helper.get_folder_ancestors(dbs, redirect_parent_id)
        cookie_path = json.dumps(ancestors)
        req.ses.cookies[PATH_COOKIE_NAME] = cookie_path
        await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=req.ses.cookies, sid=req.ses.sid))
        page = await oi.pages_helper.construct_navi_page(req.ses, cookie_path)
    else:
        page = await oi.pages_helper.construct_navi_page(req.ses, None)
    await oi.srv.client_show_page(page)

    # Notify other viewers of the destination folder (and source folder, if different)
    await oi.notify_folder_viewers(dst_folder.id, exclude_sid=req.ses.sid)
    if src_folder_id_str and int(src_folder_id_str) != dst_folder.id:
        await oi.notify_folder_viewers(int(src_folder_id_str), exclude_sid=req.ses.sid)

    return clap.Empty()


async def _recursive_set_folder_owner(dbs: orm.Session, folder_id: int, new_owner_id: str, seen: set[int], log: Logger) -> None:
    """
    Set the owner of a folder and all its subfolders + media files recursively.
    """
    assert isinstance(folder_id, int), f"Unexpected subfolder ID type on: {folder_id} ({type(folder_id)})"

    if folder_id in seen:
        log.warning(f"Folder loop detected! THIS SHOULD NOT HAPPEN. Skipping folder '{folder_id}'")
        return
    seen.add(folder_id)

    # Update folder itself
    log.debug(f"Setting owner of folder '{folder_id}' to '{new_owner_id}'")
    dbs.query(DbFolder).filter(DbFolder.id == folder_id).update({"user_id": new_owner_id})

    # Update media files in this folder
    log.debug(f"Setting owner of folder '{folder_id}' media files to '{new_owner_id}'")
    files_subq = dbs.query(DbFolderItems.media_file_id).filter(DbFolderItems.folder_id == folder_id, DbFolderItems.media_file_id != None).subquery()    # noqa: E711
    dbs.query(DbMediaFile).filter(DbMediaFile.id.in_(sqlalchemy.select(files_subq))).update({"user_id": new_owner_id})

    # Update subfolders
    sub_ids = dbs.query(DbFolderItems.subfolder_id).filter(DbFolderItems.folder_id == folder_id, DbFolderItems.subfolder_id != None).all()  # noqa: E711
    for subi in sub_ids:
        log.debug(f"Recursing to subfolder '{subi[0]}'")
        await _recursive_set_folder_owner(dbs, subi[0], new_owner_id, seen, log)


async def _cleanup_empty_user(dbs: orm.Session, user_id: str, log: Logger) -> bool:
    """
    Check if a user has any remaining content and delete them if they don't.

    A user can be safely deleted if they have:
    - No media files
    - Only an empty root folder (or no folders at all)

    If the user only has an empty root folder, we delete it first, then delete the user.
    Comments are preserved even after user deletion via the trigger
    tr_comments_set_username_on_user_delete which sets username_ifnull.

    Returns True if user was deleted, False if they still have content.
    """
    # Check if user has any media files
    media_count = dbs.query(DbMediaFile).filter(DbMediaFile.user_id == user_id).count()
    if media_count > 0:
        log.debug(f"User '{user_id}' still has {media_count} media files, not deleting")
        return False

    # Check folders - we need to be smart about root folders
    user_folders = dbs.query(DbFolder).filter(DbFolder.user_id == user_id).all()
    if not user_folders:
        # No folders at all, safe to delete user
        log.debug(f"User '{user_id}' has no folders")
    elif len(user_folders) == 1:
        # User has exactly one folder - check if it's an empty root folder
        folder = user_folders[0]

        # Check if this folder contains any items
        item_count = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder.id).count()
        if item_count > 0:
            log.debug(f"User '{user_id}' has a folder with {item_count} items, not deleting")
            return False

        # Check if this folder is a root folder (not contained in any other folder)
        is_root = dbs.query(DbFolderItems).filter(DbFolderItems.subfolder_id == folder.id).count() == 0
        if is_root:
            # This is an empty root folder, delete it first
            log.debug(f"User '{user_id}' has only an empty root folder, deleting it")
            dbs.query(DbFolder).filter(DbFolder.id == folder.id).delete()
        else:
            # This is a non-root folder, user still has content
            log.debug(f"User '{user_id}' has a non-root folder, not deleting")
            return False
    else:
        # User has multiple folders, they still have content
        log.debug(f"User '{user_id}' still has {len(user_folders)} folders, not deleting")
        return False

    # User has no content (or only had an empty root folder which we just deleted), safe to delete
    # The database trigger will handle updating comments.username_ifnull
    deleted_count = dbs.query(DbUser).filter(DbUser.id == user_id).delete()
    if deleted_count > 0:
        log.info(f"Deleted user '{user_id}' - no remaining content")
        return True
    else:
        log.warning(f"User '{user_id}' not found in database when attempting cleanup")
        return False


async def find_and_cleanup_empty_users(dbs: orm.Session, log: Logger, exclude_user_id: Optional[str] = None) -> int:
    """
    Find and clean up users who have no media files and at most one empty folder.

    This is an efficient batch operation that identifies cleanup candidates with a single query
    and then processes them. Used for manual batch cleanup operations.

    Returns the number of users that were cleaned up.
    """
    # Find users who have no media files and at most one folder
    # This is much more efficient than checking each user individually
    cleanup_candidates = dbs.execute(sqlalchemy.text("""
        SELECT u.id, u.name,
               COUNT(DISTINCT mf.id) as media_count,
               COUNT(DISTINCT f.id) as folder_count,
               MAX(f.id) as single_folder_id,
               u.created
        FROM users u
        LEFT JOIN media_files mf ON mf.user_id = u.id
        LEFT JOIN bf_folders f ON f.user_id = u.id
        WHERE u.id != COALESCE(:exclude_user_id, '')
        GROUP BY u.id, u.name, u.created
        HAVING COUNT(DISTINCT mf.id) = 0
           AND COUNT(DISTINCT f.id) <= 1
    """), {"exclude_user_id": exclude_user_id}).fetchall()

    if not cleanup_candidates:
        log.debug("No empty users found for cleanup")
        return 0

    cleaned_count = 0
    for candidate in cleanup_candidates:
        user_id = candidate.id
        folder_count = candidate.folder_count
        single_folder_id = candidate.single_folder_id

        # If user has exactly one folder, check if it's empty and a root folder
        if folder_count == 1:
            # Check if the folder contains any items
            item_count = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == single_folder_id).count()
            if item_count > 0:
                log.debug(f"User '{user_id}' has a folder with {item_count} items, not cleaning up")
                continue

            # Check if this folder is a root folder (not contained in any other folder)
            is_root = dbs.query(DbFolderItems).filter(DbFolderItems.subfolder_id == single_folder_id).count() == 0
            if not is_root:
                log.debug(f"User '{user_id}' has a non-root folder, not cleaning up")
                continue

            # Delete the empty root folder first
            log.debug(f"Deleting empty root folder {single_folder_id} for user '{user_id}'")
            dbs.query(DbFolder).filter(DbFolder.id == single_folder_id).delete()

        # Delete the user (comments will be preserved via database trigger)
        deleted_count = dbs.query(DbUser).filter(DbUser.id == user_id).delete()
        if deleted_count > 0:
            log.info(f"Cleaned up empty user '{user_id}' (had {folder_count} folders, 0 media files)")
            cleaned_count += 1
        else:
            log.warning(f"User '{user_id}' not found when attempting cleanup")

    if cleaned_count > 0:
        log.info(f"Cleaned up {cleaned_count} empty users")

    return cleaned_count


async def reorder_items_impl(oi: organizer.OrganizerInbound, req: org.ReorderItemsRequest) -> clap.Empty:
    """
    Organizer (gRPC/protobuf)
    Called when user reorders items in a folder in the client UI.
      => Use the order of items in req.ids to update the sort_order field in the database.
    """
    if not req.ids:
        oi.log.warning("reorder_items called with empty list of items. Bug in client?")
        return clap.Empty()

    if parent_folder_id := req.listing_data.get("folder_id"):
        with oi.db_new_session() as dbs:
            with dbs.begin_nested():

                # Check destination folder
                parent_folder = dbs.query(DbFolder).filter(DbFolder.id == int(parent_folder_id)).one_or_none()
                if not parent_folder:
                    raise GRPCError(GrpcStatus.NOT_FOUND, f"Parent folder {parent_folder_id} not found")
                if parent_folder.user_id != req.ses.user.id and not req.ses.is_admin:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot reorder items in another user's folder")
                reorder_is_vset = parent_folder.is_version_set

                # Reorder items
                for i, it in enumerate(req.ids):
                    if it.folder_id:
                        cnt = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == parent_folder.id, DbFolderItems.subfolder_id == int(it.folder_id)).update({"sort_order": i})
                        if cnt == 0:
                            oi.log.warning(f"DB inconsistency? Folder ID '{it.folder_id}' not in folder '{parent_folder.id}. Reordering skipped'")
                    elif it.media_file_id:
                        cnt = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == parent_folder.id, DbFolderItems.media_file_id == it.media_file_id).update({"sort_order": i})
                        if cnt == 0:
                            oi.log.warning(f"DB inconsistency? Media file ID '{it.media_file_id}' not in folder '{parent_folder.id}. Reordering skipped'")

        # A version set renumbers its vN badges from position, so the acting client must refresh too;
        # a normal folder needs no actor refresh (the drag already moved its tiles client-side, and it
        # has no server-computed labels that depend on order).
        await oi.notify_folder_viewers(int(parent_folder_id), exclude_sid=None if reorder_is_vset else req.ses.sid)
        return clap.Empty()
    else:
        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "No folder ID in UI listing, cannot reorder")

