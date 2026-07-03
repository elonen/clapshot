import json
from logging import Logger
from typing import Optional

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org

from clapshot_grpc.errors import organizer_grpc_handler
from clapshot_grpc.connect import connect_back_to_server, open_database

import sqlalchemy
from sqlalchemy import orm

from organizer.config import VERSION, MODULE_NAME, METAPLUGINS_DIR

from .migration_methods import check_migrations_impl, apply_migration_impl, db_integrity_tests
from .user_session_methods import on_start_user_session_impl, navigate_page_impl, cmd_from_client_impl
from .folder_op_methods import move_to_folder_impl, reorder_items_impl
from .testing_methods import list_tests_impl, run_test_impl
from .authz_methods import authz_user_action_impl
from .helpers.l10n import localized

from .helpers.folders import FoldersHelper
from .helpers.pages import PagesHelper
from .helpers.actiondefs import ActiondefsHelper
from .helpers.viewer_tracker import FolderViewerTracker
from . import metaplugin as mp


try:
    from typing import override  # type: ignore   # Python 3.12+
except ImportError:
    def override(func):  # type: ignore
        return func


class OrganizerInbound(org.OrganizerInboundBase):
    srv: org.OrganizerOutboundStub  # connection back to Clapshot server
    log: Logger
    db: sqlalchemy.Engine|None
    db_new_session: orm.sessionmaker     # callable session factory

    def __init__(self, logger, debug):
        self.db = None
        self.log = logger
        self.debug = debug
        self.server_info = None  # Will be set during handshake
        self.folder_viewer_tracker = FolderViewerTracker()
        self.metaplugin_loader = mp.MetaPluginLoader(METAPLUGINS_DIR, logger)
        self.metaplugin_loader.load_plugins()

    async def notify_folder_viewers(self, folder_id: int, exclude_sid: Optional[str]) -> None:
        """Send an empty ShowPage (refresh hint) to all sessions viewing folder_id, except exclude_sid."""
        for sid in self.folder_viewer_tracker.get_other_viewers(folder_id, exclude_sid):
            try:
                await self.srv.client_show_page(org.ClientShowPageRequest(sid=sid))
            except Exception as e:
                self.log.debug(f"notify_folder_viewers: failed to notify sid '{sid}': {e}")


    # Migration methods

    @override
    @organizer_grpc_handler
    async def check_migrations(self, check_migrations_request: org.CheckMigrationsRequest) -> org.CheckMigrationsResponse:
        assert self.db is None, "Database already open. Called after handshake?"
        return await check_migrations_impl(check_migrations_request, self.log)

    @override
    @organizer_grpc_handler
    async def apply_migration(self, apply_migration_request: org.ApplyMigrationRequest) -> org.ApplyMigrationResponse:
        assert self.db is None, "Database already open, cannot to apply migration. Called after handshake?"
        return await apply_migration_impl(apply_migration_request, self.log)

    @override
    async def handshake(self, server_info: org.ServerInfo) -> clap.Empty:
        """
        Receive handshake from Clapshot server.
        We must connect back to it and send handshake to establish a bidirectional connection.
        """
        self.log.debug(f"Got handshake. Server info: {json.dumps(server_info.to_dict())}")
        self.server_info = server_info

        srv_dep = org.OrganizerDependency(name="clapshot.server", min_ver=org.SemanticVersionNumber(major=0, minor=10, patch=0))
        self.srv = await connect_back_to_server(server_info, MODULE_NAME, VERSION.split("."), "Basic folders for the UI", [srv_dep], self.log)

        debug_sql = False  # set to True to log all SQL queries
        self.db, self.db_new_session = await open_database(server_info.db, debug_sql, self.log)

        self.folders_helper = FoldersHelper(self.db_new_session, self.srv, self.log)
        self.pages_helper = PagesHelper(self.folders_helper, self.srv, self.db_new_session, self.log, organizer_inbound=self)
        self.actions_helper = ActiondefsHelper()

        await db_integrity_tests(self)

        # Initialize metaplugins with context
        metaplugin_context = mp.OrganizerContext(
            db_session=self.db_new_session,
            srv=self.srv,
            log=self.log,
            folders_helper=self.folders_helper,
            pages_helper=self.pages_helper,
        )
        await self.metaplugin_loader.call_on_init_hooks(metaplugin_context)

        return clap.Empty()


    # User session methods

    @override
    @organizer_grpc_handler
    @localized
    async def on_start_user_session(self, on_start_user_session_request: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResponse:
        return await on_start_user_session_impl(self, on_start_user_session_request)

    @override
    @organizer_grpc_handler
    @localized
    async def navigate_page(self, navigate_page_request: org.NavigatePageRequest) -> org.ClientShowPageRequest:
        return await navigate_page_impl(self, navigate_page_request)

    @override
    @organizer_grpc_handler
    @localized
    async def cmd_from_client(self, cmd_from_client_request: org.CmdFromClientRequest) -> clap.Empty:
        return await cmd_from_client_impl(self, cmd_from_client_request)

    @override
    @organizer_grpc_handler
    @localized
    async def authz_user_action(self, authz_user_action_request: org.AuthzUserActionRequest) -> org.AuthzResponse:
        return await authz_user_action_impl(self, authz_user_action_request)


    # Server lifecycle events

    @override
    @organizer_grpc_handler
    async def on_media_file_ingested(self, req: org.OnMediaFileIngestedRequest) -> clap.Empty:
        from .database.operations import db_get_or_create_user_root_folder
        from .database.models import DbUser

        with self.db_new_session() as dbs:
            user = dbs.query(DbUser).filter(DbUser.id == req.user_id).one_or_none()
            if not user:
                self.log.warning(f"on_media_file_ingested: user '{req.user_id}' not found, skipping folder adoption")
                return clap.Empty()
            root_folder = await db_get_or_create_user_root_folder(
                dbs, clap.UserInfo(id=user.id, name=user.name), self.srv, self.log)
            root_folder_id = root_folder.id
            dbs.commit()

        await self.notify_folder_viewers(root_folder_id, exclude_sid=None)
        return clap.Empty()

    @override
    @organizer_grpc_handler
    async def on_media_file_deleted(self, req: org.OnMediaFileDeletedRequest) -> clap.Empty:
        # A media file was trashed server-side. Check if versions sets became empty, and repair.
        affected_parents = await self.folders_helper.repair_version_sets(req.user_id)
        for parent_id in set(affected_parents):
            await self.notify_folder_viewers(parent_id, exclude_sid=None)
        return clap.Empty()


    # Folder operation methods

    @override
    @organizer_grpc_handler
    @localized
    async def move_to_folder(self, move_to_folder_request: org.MoveToFolderRequest) -> clap.Empty:
        try:
            return await move_to_folder_impl(self, move_to_folder_request)
        except Exception:
            # The client optimistically removes the dragged item(s) from the listing (drag-and-drop).
            # On ANY failure (e.g. a rejected folder->version-set drop), re-render the client's current
            # view so the un-moved item reappears, then propagate the error (the server shows its message).
            try:
                page = await self.pages_helper.construct_navi_page(move_to_folder_request.ses, None)
                await self.srv.client_show_page(page)
            except Exception as e:
                self.log.warning(f"move_to_folder: failed to refresh client view after a move error: {e}")
            raise

    @override
    @organizer_grpc_handler
    @localized
    async def reorder_items(self, reorder_items_request: org.ReorderItemsRequest) -> clap.Empty:
        return await reorder_items_impl(self, reorder_items_request)


    # Testing methods

    @override
    @organizer_grpc_handler
    async def list_tests(self, clapshot_empty: clap.Empty) -> org.ListTestsResponse:
        return await list_tests_impl(self)

    @override
    @organizer_grpc_handler
    async def run_test(self, run_test_request: org.RunTestRequest) -> org.RunTestResponse:
        return await run_test_impl(self, run_test_request)
