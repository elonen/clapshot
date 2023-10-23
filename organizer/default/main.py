import asyncio
import json
import re
from textwrap import dedent
from grpclib import GRPCError
import grpclib.client
from grpclib.server import Server

import sys
from docopt import docopt
import clapshot_grpc.clapshot as ͼ      # most types are in this module, use ͼ for short
import clapshot_grpc.clapshot.organizer as org
from grpclib.const import Status as GrpcStatus

try:
    from typing import override  # type: ignore   # Python 3.12+
except ImportError:
    def override(func):
        return func

from logger import make_logger


# Define the version of your program
VERSION = "0.1.0"

PATH_COOKIE_NAME = "folder_path"
USER_ID_NODE_TYPE = "user_id"
FOLDER_NODE_TYPE = "folder"
PARENT_FOLDER_EDGE_TYPE = "parent_folder"
OWNER_EDGE_TYPE = "owner"


async def main():
    doc = """
    Default/example Clapshot Organizer plugin.
    This gRPC server can bind to Unix socket or TCP address.

    Usage:
      {NAME} [options] <bind>
      {NAME} (-h | --help)
      {NAME} (-v | --version)

    Required:
        <bind>              Unix socket or IP address to bind to.
                            e.g. '/tmp/organizer.sock' or '[::1]:50051'

    Options:
     -d --debug             Enable debug logging
     -j --json              Log in JSON format
     -t --tcp               Use TCP instead of Unix socket
     -h --help              Show this screen
     -v --version           Show version
    """
    global debug_logging, json_logging
    arguments = docopt(doc.format(NAME=sys.argv[0]), version=VERSION)

    bind_addr = arguments["<bind>"]
    logger=make_logger("py", debug=arguments["--debug"], json=arguments["--json"])

    server = Server([OrganizerInbound(logger)])
    if arguments["--tcp"]:
        assert re.match(r"^\d+\.\d+\.\d+\.\d+:\d+$", bind_addr) or re.match(r"^\[[0-9a-f:]*\]:\d+$", bind_addr), \
            "bind_addr must be in the format of 'ip_listen_address:port' when using TCP"
        host, port = bind_addr.split(":")
        await server.start(host=host, port=int(port))
    else:
        await server.start(path=bind_addr)  # unix socket
    logger.info(f"Organizer listening on '{bind_addr}'")

    await server.wait_closed()
    logger.info("Organizer stopped listening.")


# -------------------------------------------------------------------------------------------------

class OrganizerInbound(org.OrganizerInboundBase):
    srv: org.OrganizerOutboundStub  # connection back to Clapshot server

    def __init__(self, logger):
        self.log = logger


    @override
    async def handshake(self, server_info: org.ServerInfo) -> ͼ.Empty:
        '''
        Receive handshake from Clapshot server.
        We must connect back to it and send hanshake to establish a bidirectional connection.
        '''
        self.log.info(f"Got handshake from server.")
        self.log.debug(f"Server info: {json.dumps(server_info.to_dict())}")
        try:
            if tcp := server_info.backchannel.tcp:
                backchannel = grpclib.client.Channel(host=tcp.host, port=tcp.port)
            else:
                backchannel = grpclib.client.Channel(path=server_info.backchannel.unix.path)

            self.log.info("Connecting back to Clapshot server...")
            self.srv = org.OrganizerOutboundStub(backchannel)
            self.srv.handshake
            await self.srv.handshake(org.OrganizerInfo())
            self.log.info("Clapshot server connected.")

        except ConnectionRefusedError as e:
            self.log.error(f"Return connection to Clapshot server refused: {e}")
            raise GRPCError(GrpcStatus.UNKNOWN, "Failed to connect back to you (the Clapshot server)")
        return ͼ.Empty()


    @override
    async def on_start_user_session(self, req: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResult:
        '''
        New user session started. Send the clien a list of actions that this organizer plugin supports.
        '''
        self.log.info("on_start_user_session")

        await self.srv.client_define_actions(
            org.ClientDefineActionsRequest(
                sid = req.ses.sid,
                actions = {
                    # --- "New folder" popup ---
                    "new_folder": ͼ.ActionDef(
                        # how to display it in UI
                        ui_props=ͼ.ActionUiProps(
                            label = "New folder",
                            icon = ͼ.Icon(fa_class=ͼ.IconFaClass(classes="fa fa-folder-plus")),
                            natural_desc = "Create a new folder",
                        ),
                        # what to do when user clicks it (=show a browser dialog and make a call back to this plugin)
                        action = ͼ.ScriptCall(
                            lang = ͼ.ScriptCallLang.JAVASCRIPT,
                            code = dedent(r'''
                                    var folder_name = (await prompt("Name for the new folder", ""))?.trim();
                                    if (folder_name) { await call_organizer("new_folder", {name: folder_name}); }
                                    '''))
                    )
                }))

        return org.OnStartUserSessionResult()


    @override
    async def navigate_page(self, navigate_page_request: org.NavigatePageRequest) -> org.ClientShowPageRequest:
        self.log.info("navigate_page")
        raise GRPCError(GrpcStatus.UNIMPLEMENTED)


    @override
    async def cmd_from_client(self, cmd: org.CmdFromClientRequest) -> ͼ.Empty:
        self.log.info("cmd_from_client: " + str(cmd.to_dict()))
        raise GRPCError(GrpcStatus.UNIMPLEMENTED)


    @override
    async def authz_user_action(
        self, authz_user_action_request: org.AuthzUserActionRequest) -> org.AuthzResult:
        raise GRPCError(GrpcStatus.UNIMPLEMENTED)   # = let Clapshot server decice

    # -------------------------------------------------------------------------------------------------

    @override
    async def list_tests(self, clapshot_empty: ͼ.Empty) -> org.ListTestsResult:
        self.log.info("list_tests")
        return org.ListTestsResult(test_names=[])


    @override
    async def run_test(self, run_test_request: org.RunTestRequest) -> org.RunTestResult:
        self.log.info("run_test")
        raise GRPCError(GrpcStatus.UNIMPLEMENTED)

    # -------------------------------------------------------------------------------------------------

    async def _get_current_folder_path(self, ses: org.UserSessionData) -> list[org.PropNode]:
        '''
        User's current folder path is stored in a cookie as a JSON list of folder IDs.
        Read it, get the folder nodes from DB, and return them.
        '''
        ck = ses.cookies or {}
        try:
            if folder_ids := json.loads(ck.get(PATH_COOKIE_NAME) or '[]'):
                folder_nodes = await self.srv.db_get_prop_nodes(org.DbGetPropNodesRequest(
                    node_type = FOLDER_NODE_TYPE,
                    ids = org.IdList(ids = folder_ids)))
                if len(folder_nodes.items) == len(folder_ids):
                    return [folder_nodes.items[id] for id in folder_ids]
                else:
                    # Some folder weren't found in DB. Clear cookie.
                    await self.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies = {PATH_COOKIE_NAME: ''}, sid = ses.sid))
                    await self.srv.client_show_user_message(org.ClientShowUserMessageRequest(
                        sid = ses.sid,
                        msg = ͼ.UserMessage(
                            message = "Some unknown folder IDs in folder_path cookie. Clearing it.",
                            user_id = ses.user.id,
                            type = ͼ.UserMessageType.ERROR)))
            return []
        except json.JSONDecodeError as e:
            self.log.error(f"Failed to parse folder_path cookie: {e}. Falling back to empty folder path.")
            return []


if __name__ == '__main__':
    try:
        loop = asyncio.get_event_loop()
        loop.run_until_complete(main())
    except KeyboardInterrupt:
        print("EXIT signaled.")
