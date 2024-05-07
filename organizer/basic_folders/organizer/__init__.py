import json
from logging import Logger

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.clapshot as clap
import clapshot_grpc.clapshot.organizer as org
import sqlalchemy

from organizer.database.connection import open_database

from .migration_methods import check_migrations, apply_migration, after_migrations
from .user_session_methods import connect_back_to_server, on_start_user_session, navigate_page, cmd_from_client
from .folder_op_methods import move_to_folder, reorder_items
from .testing_methods import list_tests, run_test

from .helpers.folders import FoldersHelper
from .helpers.pages import PagesHelper
from .helpers.actiondefs import ActiondefsHelper


try:
    from typing import override  # type: ignore   # Python 3.12+
except ImportError:
    def override(func):  # type: ignore
        return func


class OrganizerInbound(org.OrganizerInboundBase):
    srv: org.OrganizerOutboundStub  # connection back to Clapshot server
    log: Logger
    db: sqlalchemy.Engine

    DbNewSession: sqlalchemy.orm.sessionmaker

    def __init__(self, logger, debug):
        self.log = logger
        self.debug = debug


    @override
    async def handshake(self, server_info: org.ServerInfo) -> clap.Empty:
        """
        Receive handshake from Clapshot server.
        We must connect back to it and send handshake to establish a bidirectional connection.
        """
        self.log.info(f"Got handshake from server.")
        self.log.debug(f"Server info: {json.dumps(server_info.to_dict())}")

        await connect_back_to_server(self, server_info)
        await open_database(self, server_info)

        self.folders_helper = FoldersHelper(self.DbNewSession, self.srv, self.log)
        self.pages_helper = PagesHelper(self.folders_helper, self.srv)
        self.actions_helper = ActiondefsHelper()

        return clap.Empty()


    # Migration methods

    @override
    async def check_migrations(self, request: org.CheckMigrationsRequest) -> org.CheckMigrationsResponse:
        return await check_migrations(self, request)

    @override
    async def apply_migration(self, request: org.ApplyMigrationRequest) -> org.ApplyMigrationResponse:
        return await apply_migration(self, request)

    @override
    async def after_migrations(self, request: org.AfterMigrationsRequest) -> clap.Empty:
        return await after_migrations(self, request)


    # User session methods

    @override
    async def on_start_user_session(self, request: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResponse:
        return await on_start_user_session(self, request)

    @override
    async def navigate_page(self, request: org.NavigatePageRequest) -> org.ClientShowPageRequest:
        return await navigate_page(self, request)

    @override
    async def cmd_from_client(self, request: org.CmdFromClientRequest) -> clap.Empty:
        return await cmd_from_client(self, request)

    @override
    async def authz_user_action(self, request: org.AuthzUserActionRequest) -> org.AuthzResponse:
        raise GRPCError(GrpcStatus.UNIMPLEMENTED)   # = let Clapshot server decide


    # Folder operation methods

    @override
    async def move_to_folder(self, request: org.MoveToFolderRequest) -> clap.Empty:
        return await move_to_folder(self, request)

    @override
    async def reorder_items(self, request: org.ReorderItemsRequest) -> clap.Empty:
        return await reorder_items(self, request)


    # Testing methods

    @override
    async def list_tests(self, request: clap.Empty) -> org.ListTestsResponse:
        return await list_tests(self)

    @override
    async def run_test(self, request: org.RunTestRequest) -> org.RunTestResponse:
        return await run_test(self, request)
