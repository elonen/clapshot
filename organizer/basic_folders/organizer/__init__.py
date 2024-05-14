import json
from logging import Logger

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.clapshot as clap
import clapshot_grpc.clapshot.organizer as org
import sqlalchemy
import sqlalchemy.exc

from functools import wraps

from organizer.database.connection import open_database

from .migration_methods import check_migrations_impl, apply_migration_impl, after_migrations_impl
from .user_session_methods import connect_back_to_server, on_start_user_session_impl, navigate_page_impl, cmd_from_client_impl
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




def organizer_grpc_handler(func):
    @wraps(func)
    async def wrapper(self, request):
        try:
            try:
                return await func(self, request)
            except sqlalchemy.exc.OperationalError as e:
                raise GRPCError(GrpcStatus.RESOURCE_EXHAUSTED, f"DB error: {e}")
        except GRPCError as e:
            # Intercept some known session errors and show them to the user nicely
            if e.status in (GrpcStatus.INVALID_ARGUMENT, GrpcStatus.PERMISSION_DENIED, GrpcStatus.ALREADY_EXISTS, GrpcStatus.RESOURCE_EXHAUSTED):
                await self.srv.client_show_user_message(org.ClientShowUserMessageRequest(sid=request.ses.sid,
                    msg = clap.UserMessage(
                        message=str(e.message),
                        user_id=request.ses.user.id,
                        type=clap.UserMessageType.ERROR,
                        details=str(e.details) if e.details else None)))
                raise GRPCError(GrpcStatus.ABORTED)   # Tell Clapshot server to ignore the result (we've shown the error to the user)
            else:
                raise e
    return wrapper



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

        await connect_back_to_server(self, server_info)
        await open_database(self, server_info)

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
