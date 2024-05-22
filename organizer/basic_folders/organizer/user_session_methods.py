from __future__ import annotations

from base64 import b64decode
import json
from grpclib import GRPCError
import grpclib
import grpclib.client
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org
from clapshot_grpc.utilities import try_send_user_message, parse_json_dict

from organizer.config import MODULE_NAME, PATH_COOKIE_NAME, VERSION
from organizer.utils import uri_arg_to_folder_path

from .database.models import DbFolder

import organizer


async def on_start_user_session_impl(oi: organizer.OrganizerInbound, req: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server when a user session is started, to define custom actions for the client.
    """
    assert req.ses.sid, "No session ID"
    await oi.srv.client_define_actions(org.ClientDefineActionsRequest(
        sid = req.ses.sid,
        actions = oi.actions_helper.make_custom_actions_map()))

    return org.OnStartUserSessionResponse()


async def navigate_page_impl(oi: organizer.OrganizerInbound, req: org.NavigatePageRequest) -> org.ClientShowPageRequest:
    """
    Organizer method (gRPC/protobuf)

    Called by the server to request organizer to construct a navigation page for the client to display.

    In contrast to individual media file players, this is the "folder view" page, which without any
    Organizer, just shows a list of all media for the user. An Organizer can define a custom
    view for this page, e.g. a folder tree or a list of categories, projects, even buttons etc.
    """
    ses = req.ses

    cookie_override = None
    try:
        cookie_override = json.dumps(uri_arg_to_folder_path(req.page_id or ""))
    except ValueError as e:
        oi.log.warning(f"Invalid folder path URI from client: '{req.page_id}'")

    if req.page_id is None:
        # When OrganizerInbound.navigate_page() is called without a page_id, it means the user has opened the main page
        # without an URL parameter => we need to clear the folder_path cookie so other handlers don't push the wrong view.
        if ses.cookies.pop(PATH_COOKIE_NAME, None):
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
        if cmd.cmd == "new_folder":
            args = parse_json_dict(cmd.args)
            parent_folder = (await oi.folders_helper.get_current_folder_path(cmd.ses, None))[-1]
            # Create folder & refresh user's view
            args = parse_json_dict(cmd.args)
            if new_folder_name := args.get("name"):
                with oi.db_new_session() as dbs:
                    new_fld = await oi.folders_helper.create_folder(dbs, cmd.ses, parent_folder, new_folder_name)
                oi.log.debug(f"Folder {new_fld.id} ('{new_fld.title}') created & committed, refreshing client's page")
                navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
                await oi.srv.client_show_page(navi_page)
            else:
                oi.log.error("new_folder command missing 'name' argument")
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "new_folder command missing 'name' argument")

        elif cmd.cmd == "open_folder":
            # Validate & parse argument JSON
            open_args = parse_json_dict(cmd.args)
            assert isinstance(open_args, dict), "open_folder argument not a dict"
            folder_id = open_args.get("id")
            assert folder_id, "open_folder arg 'id' missing"
            assert isinstance(folder_id, int), "open_folder arg 'id' not an int"

            # Construct new breadcrumb trail
            trail = [f.id for f in (await oi.folders_helper.get_current_folder_path(cmd.ses, None))]
            if folder_id in trail:
                trail = trail[:trail.index(folder_id)+1] # go up in current trail => remove all after this folder
            else:
                trail.append(folder_id) # add folder id at the end

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
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "rename_folder command missing 'id' or 'new_name' argument")
            folder_id = int(args["id"])
            with oi.db_new_session() as dbs:
                with dbs.begin_nested():
                    fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                    if not fld:
                        raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{args['id']}' not found")
                    if fld.user_id not in (cmd.ses.user.id,"admin"):
                        raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot rename another user's folder")
                    fld.title = args["new_name"]

            oi.log.debug(f"Renamed folder '{fld.id}' to '{fld.title}'")
            page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
            await oi.srv.client_show_page(page)

        elif cmd.cmd == "trash_folder":
            args = parse_json_dict(cmd.args) # {"id": 123}
            if not args or not args.get("id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "trash_folder command missing 'id' argument")
            folder_id = int(args["id"])

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

        else:
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, f"Unknown organizer command: {cmd.cmd}")

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
