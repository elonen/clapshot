from __future__ import annotations

import json
from typing import Optional
import sqlalchemy
from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org
from clapshot_grpc.utilities import try_send_user_message, parse_json_dict

from organizer.config import PATH_COOKIE_NAME
from organizer.helpers.folders import SHARED_FOLDER_TOKEN_COOKIE_NAME
from organizer.utils import uri_arg_to_folder_path

from .helpers.l10n import _, ngettext

from .database.models import DbFolder, DbFolderItems, DbMediaFile, FolderKind

import organizer


def _int_arg(args: dict, key: str) -> int:
    """
    Parse a required integer command argument. Maps missing/non-numeric client input to a clean
    INVALID_ARGUMENT instead of an uncaught ValueError (which would bypass the friendly-error path).
    """
    try:
        return int(args[key])
    except (KeyError, TypeError, ValueError):
        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("Command argument '{key}' must be an integer").format(key=key))


async def define_session_actions(oi: organizer.OrganizerInbound, ses: org.UserSessionData) -> None:
    """(Re)define the client's actions for this session. Action labels are localized to the
    current request locale, so this is re-sent on navigation (on_start runs before the client
    has reported its language; navigate_page re-sends localized labels)."""
    actions = oi.actions_helper.make_custom_actions_map()
    actions = oi.metaplugin_loader.call_extend_actions_hooks(actions)
    await oi.srv.client_define_actions(org.ClientDefineActionsRequest(sid=ses.sid, actions=actions))


async def on_start_user_session_impl(oi: organizer.OrganizerInbound, req: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server when a user session is started, to define custom actions for the client.
    """
    assert req.ses.sid, "No session ID"

    # Housekeeping for version set integrity, just in case
    await oi.folders_helper.repair_version_sets(req.ses.user.id)

    await define_session_actions(oi, req.ses)

    return org.OnStartUserSessionResponse()

async def navigate_page_impl(oi: organizer.OrganizerInbound, req: org.NavigatePageRequest) -> org.ClientShowPageRequest:
    """
    Organizer method (gRPC/protobuf)

    Server calls this to request Organizer to construct a navigation page for the Client to show.

    This is a "folder view" page (not a media player). Without an Organizer, the Server would just show
    a list of all media for the user. An Organizer can define a custom view,
    e.g. a folder tree or a list of categories, projects, even buttons etc.
    """
    ses = req.ses

    # Re-send actions localized to the session language (now known, unlike at on_start).
    await define_session_actions(oi, ses)

    # If page ID starts with "shared.", e.g. ?p=shared.ABCD1234, it means the user has opened
    # a shared folder link.
    if req.page_id and req.page_id.startswith("shared."):
        share_token = req.page_id.split(".", 1)[1]
        with oi.db_new_session() as dbs:
            if share := await oi.folders_helper.get_share_by_token(dbs, share_token):
                req.page_id = str(share.folder_id)  # Replace token with folder ID for further processing
                ses.cookies[PATH_COOKIE_NAME] = json.dumps([share.folder_id])   # Use it as a new folder path root
                await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=ses.cookies, sid=ses.sid))

                # If current user is not the owner of the shared folder, store token in a cookie
                owner = await oi.folders_helper.get_folder_owner(dbs, share.folder_id)
                if owner and owner.id != ses.user.id:
                    ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] = share_token
                    await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=ses.cookies, sid=ses.sid))
            else:
                # Token not found? Reset session.
                await try_send_user_message(oi.srv,
                    org.ClientShowUserMessageRequest(sid=ses.sid,
                        msg=clap.UserMessage(
                            message=_("This shared folder link is invalid or has been revoked"),
                            type=clap.UserMessageType.ERROR)))
                ses.cookies.pop(SHARED_FOLDER_TOKEN_COOKIE_NAME, None)
                ses.cookies.pop(PATH_COOKIE_NAME, None)
                await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=ses.cookies, sid=ses.sid))
                return await oi.pages_helper.construct_navi_page(ses, None)

    # Normal folder navigation, e.g. ?p=1.2.3
    cookie_override: Optional[str] = None
    if req.page_id:
        try:
            cookie_override = json.dumps(uri_arg_to_folder_path(req.page_id))
        except ValueError:
            oi.log.warning(f"Invalid folder path URI from client: '{req.page_id}'")
    else:
        # When OrganizerInbound.navigate_page() is called without a page_id, it means the user has opened the main page
        # without an URL parameter => we need to clear the folder_path cookie so other handlers don't push the wrong view.
        ses.cookies.pop(PATH_COOKIE_NAME, None)
        await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies=ses.cookies, sid=ses.sid))

    return await oi.pages_helper.construct_navi_page(ses, cookie_override)


async def cmd_from_client_impl(oi: organizer.OrganizerInbound, cmd: org.CmdFromClientRequest) -> clap.Empty:
    """
    Organizer method (gRPC/protobuf)

    These are usually triggered by user actions in the UI, and defined by the Organizer
    when a user session is started.

    The client doesn't really know what these commands do, it just executes action scripts
    that the organizer plugin has defined, e.g. for popup menus. The scripts can be anything,
    but they usually call these methods with the appropriate arguments.

    => These command names are organizer-specific and could be named anything.
    """
    try:
        args = parse_json_dict(cmd.args)

        # Try metaplugins first
        if await oi.metaplugin_loader.call_handle_custom_command_hooks(cmd.cmd, args, cmd.ses, oi):
            return clap.Empty()

        if cmd.cmd == "new_folder":
            folder_path, _root = await oi.folders_helper.get_current_folder_path(cmd.ses, None)
            parent_folder = folder_path[-1]  # create the subfolder in the folder currently being viewed
            # Create folder & refresh user's view
            if new_folder_name := args.get("name"):
                with oi.db_new_session() as dbs:
                    new_fld = await oi.folders_helper.create_folder(dbs, cmd.ses, parent_folder, new_folder_name)
                oi.log.debug(f"Folder {new_fld.id} ('{new_fld.title}') created & committed, refreshing client's page")
                navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
                await oi.srv.client_show_page(navi_page)
                await oi.notify_folder_viewers(parent_folder.id, exclude_sid=cmd.ses.sid)
            else:
                oi.log.error("new_folder command missing 'name' argument")
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("new_folder command missing 'name' argument"))

        elif cmd.cmd == "set_folder_kind":
            # Convert folder to version set or back to normal.
            # {"id": 123, "kind": "version_set"|"normal"}
            args = parse_json_dict(cmd.args)
            if not args or not args.get("id") or not args.get("kind"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("set_folder_kind requires 'id' and 'kind'"))
            vs_folder_id = _int_arg(args, "id")
            new_kind = str(args["kind"])
            if new_kind not in (FolderKind.NORMAL.value, FolderKind.VERSION_SET.value):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("Invalid folder kind: {kind}").format(kind=new_kind))

            with oi.db_new_session() as dbs:
                fld = dbs.query(DbFolder).filter(DbFolder.id == vs_folder_id).one_or_none()
                if not fld:
                    raise GRPCError(GrpcStatus.NOT_FOUND, _("Folder ID '{id}' not found").format(id=vs_folder_id))
                if fld.user_id != cmd.ses.user.id and not cmd.ses.is_admin:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, _("Cannot change another user's folder"))

                if fld.kind == new_kind:
                    oi.log.debug(f"Folder {fld.id} already has kind '{new_kind}', no change needed")
                    return clap.Empty()
                elif new_kind == FolderKind.VERSION_SET.value:
                    has_parent = dbs.query(DbFolderItems).filter(DbFolderItems.subfolder_id == fld.id).first() is not None
                    if not has_parent:
                        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("Cannot turn the root folder into a version set"))

                    contents = await oi.folders_helper.fetch_folder_contents(fld, cmd.ses)
                    if any(isinstance(it, DbFolder) for it in contents):
                        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("Cannot make a version set from a folder that contains subfolders"))

                    media = [it for it in contents if isinstance(it, DbMediaFile)]
                    if not media:
                        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("A version set needs at least one media file"))

                    with dbs.begin_nested():
                        fld.kind = FolderKind.VERSION_SET.value
                        fld.active_media_file_id = media[0].id
                else:
                    with dbs.begin_nested():
                        fld.kind = FolderKind.NORMAL.value
                        fld.active_media_file_id = None

            page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(page)
            await oi.notify_folder_viewers(vs_folder_id, exclude_sid=cmd.ses.sid)

        elif cmd.cmd == "make_versioned":
            # Convert a selection of media files into a version set.
            # (This is
            # {"ids": [{"mediaFileId": ...} | {"folderId": ...}]}
            args = parse_json_dict(cmd.args)
            raw_ids = args.get("ids") or []

            if any(x.get("folderId") for x in raw_ids):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("Make Versioned works on media files only; deselect folders"))

            sel_media_ids = [x.get("mediaFileId") for x in raw_ids if x.get("mediaFileId")]
            if not sel_media_ids:
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("Select at least one media file to make versioned"))

            with oi.db_new_session() as dbs:
                parent_item = dbs.query(DbFolderItems).filter(DbFolderItems.media_file_id == sel_media_ids[0]).one_or_none()
                if not parent_item or parent_item.folder_id is None:
                    raise GRPCError(GrpcStatus.NOT_FOUND, _("Selected media file is not in any folder"))
                parent_id = parent_item.folder_id
                parent_fld = dbs.query(DbFolder).filter(DbFolder.id == parent_id).one()
                if parent_fld.user_id != cmd.ses.user.id and not cmd.ses.is_admin:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, _("Cannot modify another user's folder"))

                # Order the selected media by their current display order in the parent (leftmost first).
                sel_set = set(sel_media_ids)
                parent_contents = await oi.folders_helper.fetch_folder_contents(parent_fld, cmd.ses)
                ordered = [it for it in parent_contents if isinstance(it, DbMediaFile) and it.id in sel_set]

                # Since we are ordering them the same as in current folder, all all selected files
                # # must live in this one parent
                if len(ordered) != len(sel_set):
                    raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("All selected media files must be in the same folder to group them into a version set"))

                title = ordered[0].title or _("Version Set")
                with dbs.begin_nested():
                    new_fld = DbFolder(user_id=parent_fld.user_id, title=title, kind=FolderKind.VERSION_SET.value)
                    dbs.add(new_fld)
                    dbs.flush()

                    max_so = dbs.query(sqlalchemy.func.max(DbFolderItems.sort_order)).filter(DbFolderItems.folder_id == parent_id).scalar() or 0
                    dbs.add(DbFolderItems(folder_id=parent_id, subfolder_id=new_fld.id, sort_order=max_so + 1))
                    for i, m in enumerate(ordered):
                        dbs.query(DbFolderItems).filter(DbFolderItems.media_file_id == m.id).update({"folder_id": new_fld.id, "sort_order": i})
                    new_fld.active_media_file_id = ordered[0].id

            page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(page)
            await oi.notify_folder_viewers(parent_id, exclude_sid=cmd.ses.sid)

        elif cmd.cmd == "set_active_version":
            # {"folder_id": ..., "media_file_id": ...} -- set the version set's active version.
            # Invoked both by the "Set Active Ver" popup and by the player's version dropdown.
            args = parse_json_dict(cmd.args)
            if not args or not args.get("folder_id") or not args.get("media_file_id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("set_active_version requires 'folder_id' and 'media_file_id'"))

            sa_folder_id = _int_arg(args, "folder_id")
            sa_media_id = str(args["media_file_id"])

            with oi.db_new_session() as dbs:
                fld = dbs.query(DbFolder).filter(DbFolder.id == sa_folder_id).one_or_none()
                if not fld:
                    raise GRPCError(GrpcStatus.NOT_FOUND, _("Folder ID '{id}' not found").format(id=sa_folder_id))
                if fld.user_id != cmd.ses.user.id and not cmd.ses.is_admin:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, _("Cannot change another user's version set"))
                member = dbs.query(DbFolderItems).filter(
                    DbFolderItems.folder_id == sa_folder_id,
                    DbFolderItems.media_file_id == sa_media_id).first()
                if not member:
                    raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("That media file is not in this version set"))
                with dbs.begin_nested():
                    fld.active_media_file_id = sa_media_id

            # Push a notify only, not full ShowPage, because set_active_version could be called from
            # player dropdown, where we want to avoid interrupting playback.
            await oi.notify_folder_viewers(sa_folder_id, exclude_sid=None)

        elif cmd.cmd == "open_folder":
            # Validate & parse argument JSON
            open_args = parse_json_dict(cmd.args)
            assert isinstance(open_args, dict), "open_folder argument not a dict"
            folder_id = open_args.get("id")
            assert folder_id, "open_folder arg 'id' missing"
            assert isinstance(folder_id, int), "open_folder arg 'id' not an int"

            # Check if target folder exists and get its owner
            with oi.db_new_session() as dbs:
                target_folder = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                if not target_folder:
                    raise GRPCError(GrpcStatus.NOT_FOUND, _("Folder ID '{id}' not found").format(id=folder_id))
                target_owner_id = target_folder.user_id

            # Construct new breadcrumb trail
            folder_path, _root_folder = await oi.folders_helper.get_current_folder_path(cmd.ses, None)
            trail = [f.id for f in folder_path]

            if folder_id in trail:
                # Going back up in current trail => remove all after this folder
                trail = trail[:trail.index(folder_id)+1]
            else:
                # Check if we're crossing ownership boundaries
                current_owner_id = folder_path[-1].user_id if folder_path else None

                if (current_owner_id != target_owner_id and
                    target_owner_id != cmd.ses.user.id and
                    cmd.ses.is_admin):
                    # Admin is switching to another user's folder - start fresh trail from target
                    oi.log.debug(f"Admin switching from {current_owner_id} to {target_owner_id} folders - starting fresh trail")
                    trail = [folder_id]
                else:
                    # Normal case: append folder id at the end
                    trail.append(folder_id)

            # Update folder path cookie
            serialized_trail = json.dumps(trail)
            cmd.ses.cookies[PATH_COOKIE_NAME] = serialized_trail
            oi.log.debug(f"Setting new folder_path cookie: {serialized_trail}")
            await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(
                cookies = cmd.ses.cookies,
                sid = cmd.ses.sid))

            # Update page to view the opened folder
            page = await oi.pages_helper.construct_navi_page(cmd.ses, serialized_trail)
            await oi.srv.client_show_page(page)

        elif cmd.cmd == "rename_folder":
            args = parse_json_dict(cmd.args)  # {"id": 123, "new_name": "New name"}
            if not args or not args.get("id") or not args.get("new_name"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("rename_folder command missing 'id' or 'new_name' argument"))
            folder_id = _int_arg(args, "id")
            with oi.db_new_session() as dbs:
                fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                if not fld:
                    raise GRPCError(GrpcStatus.NOT_FOUND, _("Folder ID '{id}' not found").format(id=args['id']))

                # Check authorization via metaplugins + default checks
                from .authz_methods import check_action_authorization
                await check_action_authorization(oi, "rename_folder", folder=fld, ses=cmd.ses)

                with dbs.begin_nested():
                    fld.title = args["new_name"]

            oi.log.debug(f"Renamed folder '{fld.id}' to '{fld.title}'")
            page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(page)
            await oi.notify_folder_viewers(fld.id, exclude_sid=cmd.ses.sid)

        elif cmd.cmd == "trash_folder":
            args = parse_json_dict(cmd.args) # {"id": 123}
            if not args or not args.get("id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("trash_folder command missing 'id' argument"))
            folder_id = _int_arg(args, "id")

            # Get folder and check authorization
            from .authz_methods import check_action_authorization
            with oi.db_new_session() as dbs:
                fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                if not fld:
                    raise GRPCError(GrpcStatus.NOT_FOUND, _("Folder ID '{id}' not found").format(id=folder_id))
                await check_action_authorization(oi, "trash_folder", folder=fld, ses=cmd.ses)

            # Remember parent folder for notifying other viewers
            folder_path, _root = await oi.folders_helper.get_current_folder_path(cmd.ses, None)
            parent_folder_id = folder_path[-2].id if len(folder_path) > 1 else folder_path[-1].id

            # Delete the folder and its contents, gather media file IDs to delete later (after transaction, to avoid DB locks)
            media_to_delete = []
            with oi.db_new_session() as dbs:
                with dbs.begin_nested():
                    media_to_delete = await oi.folders_helper.trash_folder_recursive(dbs, folder_id, cmd.ses)

            # Trash the media files
            for vi in media_to_delete:
                oi.log.debug(f"Trashing media file '{vi}'")
                await oi.srv.delete_media_file(org.DeleteMediaFileRequest(id=vi))  # this cleans up the media's files on disk, too

            page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(page)
            await oi.notify_folder_viewers(parent_folder_id, exclude_sid=cmd.ses.sid)

        elif cmd.cmd == "share_folder":
            args = parse_json_dict(cmd.args)
            if not args or not args.get("id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("share_folder command missing 'id' argument"))

            folder_id = _int_arg(args, "id")

            with oi.db_new_session() as dbs:
                shared = await oi.folders_helper.create_folder_share(dbs, folder_id, cmd.ses)
                folder_title = str(shared.folder.title)
                dbs.commit()

                # Generate shareable URL using server_url_base from server_info
                if not oi.server_info or not oi.server_info.url_base:
                    raise GRPCError(GrpcStatus.FAILED_PRECONDITION, _("Server URL base not configured - cannot generate shareable URLs"))

            # Update UI after transaction commit
            navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(navi_page)
            await oi.notify_folder_viewers(folder_id, exclude_sid=cmd.ses.sid)

            # Show message with share URL
            await try_send_user_message(oi.srv,
                org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                    msg=clap.UserMessage(
                        message=_("Folder shared. Use popup/'Copy URL' to get a link."),
                        details= _("Folder sharing token created for '{title}'.").format(title=folder_title),
                        type=clap.UserMessageType.OK)))

        elif cmd.cmd == "revoke_share":
            # Parse arguments
            args = parse_json_dict(cmd.args)
            if not args or not args.get("id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("revoke_share command missing 'id' argument"))

            folder_id = _int_arg(args, "id")

            with oi.db_new_session() as dbs:
                # Revoke the share
                revoked = await oi.folders_helper.revoke_folder_share(dbs, folder_id, cmd.ses)
                dbs.commit()

            # Update UI after transaction commit
            navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(navi_page)
            await oi.notify_folder_viewers(folder_id, exclude_sid=cmd.ses.sid)

            # Show success message
            if revoked:
                await try_send_user_message(oi.srv,
                    org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                        msg=clap.UserMessage(
                            message=_("Folder sharing has been revoked"),
                            type=clap.UserMessageType.OK)))
            else:
                await try_send_user_message(oi.srv,
                    org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                        msg=clap.UserMessage(
                            message=_("This folder is not currently shared"),
                            type=clap.UserMessageType.ERROR)))

        elif cmd.cmd == "cleanup_empty_user":
            # Parse arguments
            args = parse_json_dict(cmd.args)
            if not args or not args.get("folder_id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("cleanup_empty_user command missing 'folder_id' argument"))

            if not cmd.ses.is_admin:
                raise GRPCError(GrpcStatus.PERMISSION_DENIED, _("Only admin can clean up users"))

            folder_id_str = str(args["folder_id"])

            # Check if this is a batch cleanup request (folder_id = '*')
            if folder_id_str == "*":
                # Batch cleanup all empty users (excluding the admin user who is performing the action)
                folder_path, _root = await oi.folders_helper.get_current_folder_path(cmd.ses, None)
                cur_folder_id = folder_path[-1].id  # the folder the admin is currently viewing
                with oi.db_new_session() as dbs:
                    from .folder_op_methods import find_and_cleanup_empty_users
                    cleaned_count = await find_and_cleanup_empty_users(dbs, oi.log, exclude_user_id=cmd.ses.user.id)
                    dbs.commit()

                # Update UI after transaction commit
                navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
                await oi.srv.client_show_page(navi_page)
                await oi.notify_folder_viewers(cur_folder_id, exclude_sid=cmd.ses.sid)

                # Show result message
                if cleaned_count > 0:
                    await try_send_user_message(oi.srv,
                        org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                            msg=clap.UserMessage(
                                message=ngettext("Cleaned up {count} empty user", "Cleaned up {count} empty users", cleaned_count).format(count=cleaned_count),
                                details=_("Comments from deleted users are preserved but marked as from deleted users."),
                                type=clap.UserMessageType.OK)))
                else:
                    await try_send_user_message(oi.srv,
                        org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                            msg=clap.UserMessage(
                                message=_("No empty users found to clean up"),
                                type=clap.UserMessageType.OK)))
            else:
                # Single user cleanup (existing logic)
                folder_id = int(folder_id_str)

                # Find the user who owns this folder
                with oi.db_new_session() as dbs:
                    target_folder = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                    if not target_folder:
                        raise GRPCError(GrpcStatus.NOT_FOUND, _("Folder ID '{id}' not found").format(id=folder_id))

                    user_id = target_folder.user_id
                    from .folder_op_methods import _cleanup_empty_user
                    was_deleted = await _cleanup_empty_user(dbs, user_id, oi.log)
                    dbs.commit()

                # Update UI after transaction commit
                navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
                await oi.srv.client_show_page(navi_page)
                await oi.notify_folder_viewers(folder_id, exclude_sid=cmd.ses.sid)

                # Show result message
                if was_deleted:
                    await try_send_user_message(oi.srv,
                        org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                            msg=clap.UserMessage(
                                message=_("User '{id}' has been cleaned up (deleted)").format(id=user_id),
                                details=_("Comments from this user are preserved but marked as from a deleted user."),
                                type=clap.UserMessageType.OK)))
                else:
                    await try_send_user_message(oi.srv,
                        org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                            msg=clap.UserMessage(
                                message=_("User '{id}' was not deleted").format(id=user_id),
                                details=_("User still has media files or non-empty folders."),
                                type=clap.UserMessageType.OK)))

        else:
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, _("Unknown organizer command: {cmd}").format(cmd=cmd.cmd))

    except GRPCError as e:

        # Intercept some known session errors and show them to the user nicely
        if e.status in (GrpcStatus.INVALID_ARGUMENT, GrpcStatus.PERMISSION_DENIED, GrpcStatus.ALREADY_EXISTS):
            if err := await try_send_user_message(oi.srv,
                    org.ClientShowUserMessageRequest(sid=cmd.ses.sid,
                        msg=clap.UserMessage(
                            message=str(e.message),
                            user_id=cmd.ses.user.id,
                            type=clap.UserMessageType.ERROR,
                            details=str(e.details) if e.details else None))):
                oi.log.error(f"Error calling client_show_user_message(): {err}")
        else:
            raise e

    return clap.Empty()
