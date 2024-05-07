import json
from grpclib import GRPCError
import grpclib
import grpclib.client
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.clapshot as clap
import clapshot_grpc.clapshot.organizer as org

from organizer.config import MODULE_NAME, PATH_COOKIE_NAME, VERSION
from organizer.utils import parse_json_args

from .database.models import DbFolder



async def on_start_user_session(oi, req: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server when a user session is started, to define custom actions for the client.
    """
    oi.log.info("on_start_user_session")
    assert req.ses.sid, "No session ID"
    await oi.srv.client_define_actions(org.ClientDefineActionsRequest(
        sid = req.ses.sid,
        actions = oi.actions_helper.make_custom_actions_map()))

    return org.OnStartUserSessionResponse()


async def navigate_page(oi, req: org.NavigatePageRequest) -> org.ClientShowPageRequest:
    """
    Organizer method (gRPC/protobuf)

    Called by the server to request organizer to construct a navigation page for the client to display.

    In contrast to individual video pages, this is the "folder view" page, which without any
    Organizer, just shows a list of all videos for the user. An Organizer can define a custom
    view for this page, e.g. a folder tree or a list of categories, projects, even buttons etc.
    """
    ses = req.ses
    return await oi.pages_helper.construct_navi_page(ses, None)


async def cmd_from_client(oi, cmd: org.CmdFromClientRequest) -> clap.Empty:
    """
    Organizer method (gRPC/protobuf)

    These are usually triggered by user actions in the UI, and defined by the Organizer
    when a user session is started.

    The client doesn't really know what these commands do, it just executes action scripts
    that the organizer plugin has defined, e.g. for popup menus. The scripts can be anything,
    but they usually call these methods with the appropriate arguments.

    => These command names are organizer-specific and could be named anything.
    """
    if cmd.cmd == "new_folder":
        args = parse_json_args(cmd.args)
        parent_folder = (await oi.folders_helper.get_current_folder_path(cmd.ses, None))[-1]
        with oi.DbNewSession() as dbs:
            try:
                # Create folder & refresh user's view
                args = parse_json_args(cmd.args)
                if new_folder_name := args.get("name"):
                    new_fld = await oi.folders_helper.create_folder(dbs, cmd.ses, parent_folder, new_folder_name)
                    oi.log.debug(f"Folder {new_fld.id} ('{new_fld.title}') created & committed, refreshing client's page")
                    navi_page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
                    await oi.srv.client_show_page(navi_page)
                else:
                    oi.log.error("new_folder command missing 'name' argument")
                    raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "new_folder command missing 'name' argument")
            except Exception as e:
                oi.log.error(f"Failed to create folder: {e}")
                raise GRPCError(GrpcStatus.INTERNAL, "Failed to create folder")

    elif cmd.cmd == "open_folder":
        # Validate & parse argument JSON
        open_args = parse_json_args(cmd.args)
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
        new_cookie = json.dumps(trail)
        oi.log.debug(f"Setting new folder_path cookie: {new_cookie}")
        await oi.srv.client_set_cookies(org.ClientSetCookiesRequest(
            cookies = {PATH_COOKIE_NAME: new_cookie},
            sid = cmd.ses.sid))

        # Update page to view the opened folder
        page = await oi.pages_helper.construct_navi_page(cmd.ses, new_cookie)
        await oi.srv.client_show_page(page)

    elif cmd.cmd == "rename_folder":
        args = parse_json_args(cmd.args)  # {"id": 123, "new_name": "New name"}
        if not args or not args.get("id") or not args.get("new_name"):
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "rename_folder command missing 'id' or 'new_name' argument")
        folder_id = int(args["id"])
        with oi.DbNewSession() as dbs:
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
        args = parse_json_args(cmd.args) # {"id": 123}
        if not args or not args.get("id"):
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "trash_folder command missing 'id' argument")
        folder_id = int(args["id"])

        # Delete the folder and its contents, gather video IDs to delete later (after transaction, to avoid DB locks)
        videos_to_delete = []
        with oi.DbNewSession() as dbs:
            with dbs.begin_nested():
                videos_to_delete = await oi.folders_helper.trash_folder_recursive(dbs, folder_id, cmd.ses.user.id)

        # Trash the videos
        for vi in videos_to_delete:
            oi.log.debug(f"Trashing video '{vi}'")
            await oi.srv.delete_video(org.DeleteVideoRequest(id=vi))  # this cleans up the video's files, too

        page = await oi.pages_helper.construct_navi_page(cmd.ses, None)
        await oi.srv.client_show_page(page)

    else:
        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, f"Unknown organizer command: {cmd.cmd}")

    return clap.Empty()


async def connect_back_to_server(oi, server_info: org.ServerInfo):
    """
    Helper. Connect back to the Clapshot server, using the TCP or Unix socket address provided in the handshake.
    """
    try:
        if tcp := server_info.backchannel.tcp:
            backchannel = grpclib.client.Channel(host=tcp.host, port=tcp.port)
        else:
            backchannel = grpclib.client.Channel(path=server_info.backchannel.unix.path)

        oi.log.info("Connecting back to Clapshot server...")
        oi.srv = org.OrganizerOutboundStub(backchannel)
        await oi.srv.handshake(org.OrganizerInfo(
            version=org.SemanticVersionNumber(major=int(VERSION.split(".")[0]), minor=int(VERSION.split(".")[1]), patch=int(VERSION.split(".")[2])),
            name=MODULE_NAME,
            description="Basic folders for the UI",
            hard_dependencies=[
                org.OrganizerDependency(
                    name="clapshot.server",
                    min_ver=org.SemanticVersionNumber(major=0, minor=5, patch=6))
            ],
        ))
        oi.log.info("Clapshot server connected.")

    except ConnectionRefusedError as e:
        oi.log.error(f"Return connection to Clapshot server refused: {e}")
        raise GRPCError(GrpcStatus.UNKNOWN, "Failed to connect back to you (the Clapshot server)")
