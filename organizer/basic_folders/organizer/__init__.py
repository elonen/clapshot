import json
from logging import Logger
import traceback

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org

from clapshot_grpc.errors import organizer_grpc_handler
from clapshot_grpc.connect import connect_back_to_server, open_database

import sqlalchemy
import sqlalchemy.exc

from functools import wraps

from organizer.config import VERSION, MODULE_NAME

from .migration_methods import check_migrations_impl, apply_migration_impl, after_migrations_impl
from .user_session_methods import on_start_user_session_impl, navigate_page_impl, cmd_from_client_impl
from .folder_op_methods import move_to_folder_impl, reorder_items_impl
from .testing_methods import list_tests_impl, run_test_impl

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
    db_new_session: sqlalchemy.orm.sessionmaker     # callable session factory

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

        srv_dep = org.OrganizerDependency(name="clapshot.server", min_ver=org.SemanticVersionNumber(major=0, minor=6, patch=0))
        self.srv = await connect_back_to_server(server_info, MODULE_NAME, VERSION.split("."), "Basic folders for the UI", [srv_dep], self.log)

        debug_sql = False  # set to True to log all SQL queries
        self.db, self.db_new_session = await open_database(server_info, debug_sql, self.log)

        self.folders_helper = FoldersHelper(self.db_new_session, self.srv, self.log)
        self.pages_helper = PagesHelper(self.folders_helper, self.srv, self.db_new_session, self.log)
        self.actions_helper = ActiondefsHelper()

        return clap.Empty()


    # Migration methods

    @override
    @organizer_grpc_handler
    async def check_migrations(self, request: org.CheckMigrationsRequest) -> org.CheckMigrationsResponse:
        return await check_migrations_impl(self, request)

    @override
    @organizer_grpc_handler
    async def apply_migration(self, request: org.ApplyMigrationRequest) -> org.ApplyMigrationResponse:
        return await apply_migration_impl(self, request)

    @override
    @organizer_grpc_handler
    async def after_migrations(self, request: org.AfterMigrationsRequest) -> clap.Empty:
        return await after_migrations_impl(self, request)


    # User session methods

    @override
    @organizer_grpc_handler
    async def on_start_user_session(self, request: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResponse:
        return await on_start_user_session_impl(self, request)

    @override
    @organizer_grpc_handler
    async def navigate_page(self, request: org.NavigatePageRequest) -> org.ClientShowPageRequest:
        return await navigate_page_impl(self, request)

    @override
    @organizer_grpc_handler
    async def cmd_from_client(self, request: org.CmdFromClientRequest) -> clap.Empty:
        return await cmd_from_client_impl(self, request)

    @override
    @organizer_grpc_handler
    async def authz_user_action(self, request: org.AuthzUserActionRequest) -> org.AuthzResponse:
        raise GRPCError(GrpcStatus.UNIMPLEMENTED)   # = let Clapshot server decide


    # Folder operation methods

    @override
    @organizer_grpc_handler
    async def move_to_folder(self, request: org.MoveToFolderRequest) -> clap.Empty:
        return await move_to_folder_impl(self, request)

    @override
    @organizer_grpc_handler
    async def reorder_items(self, request: org.ReorderItemsRequest) -> clap.Empty:
        return await reorder_items_impl(self, request)


    # Testing methods

    @override
    @organizer_grpc_handler
    async def list_tests(self, request: clap.Empty) -> org.ListTestsResponse:
        return await list_tests_impl(self)

    @override
    @organizer_grpc_handler
    async def run_test(self, request: org.RunTestRequest) -> org.RunTestResponse:
        return await run_test_impl(self, request)
