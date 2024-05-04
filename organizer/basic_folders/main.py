import asyncio
import json
import sys
import re

from typing import Optional

import sqlalchemy.log
from logger import make_logger
from textwrap import dedent
from html import escape as html_escape
from docopt import docopt
from logging import Logger

from grpclib import GRPCError
import grpclib.client
from grpclib.server import Server
from grpclib.const import Status as GrpcStatus

import sqlalchemy
from sqlalchemy.orm import sessionmaker, Session

import clapshot_grpc.clapshot as clap
import clapshot_grpc.clapshot.organizer as org

try:
    from typing import override  # type: ignore   # Python 3.12+
except ImportError:
    def override(func):  # type: ignore
        return func

from database.operations import db_check_and_fix_integrity, db_check_pending_migrations, db_apply_migration, db_test_orm_mappings, db_get_or_create_user_root_folder
from database.models import DbFolder, DbFolderItems, DbVideo


VERSION = "0.6.0"
MODULE_NAME = "clapshot.organizer.basic_folders"
PATH_COOKIE_NAME = "folder_path"


async def main():
    doc = """
    Clapshot Organizer plugin that implements basic folders for the UI.

    This gRPC server can bind to Unix socket or TCP address, and is typically
    executed by the Clapshot server as a subprocess, in which case the server also
    provides the `bind` argument. You can, however, also run it manually and
    configure the server to connect to it via TCP.

    Usage:
      {NAME} [options] <bind>
      {NAME} (-h | --help)
      {NAME} (-v | --version)

    Required:
        <bind>              Unix socket or IP address to bind to.
                            e.g. '/tmp/organizer.sock', '[::1]:50051', '127.0.0.1:50051'

    Options:
     -d --debug             Enable debug logging
     -j --json              Log in JSON format
     -t --tcp               Use TCP instead of Unix socket
     -h --help              Show this screen
     -v --version           Show version
    """
    global debug_logging, json_logging
    arguments = docopt(doc.format(NAME=sys.argv[0]), version=VERSION)
    flag_debug, flag_json, flag_tcp = arguments["--debug"], arguments["--json"], arguments["--tcp"]

    bind_addr = arguments["<bind>"]
    logger=make_logger("bf", debug=flag_debug, json=flag_json)

    server = Server([OrganizerInbound(logger, flag_debug)])
    if flag_tcp:
        assert re.match(r"^\d+\.\d+\.\d+\.\d+:\d+$", bind_addr) or re.match(r"^\[[0-9a-f:]*\]:\d+$", bind_addr), \
            "bind_addr must be in the format of 'ip_listen_address:port' when using TCP"
        host, port = bind_addr.rsplit(":", 1)
        await server.start(host=host, port=int(port))
    else:
        await server.start(path=bind_addr)  # unix socket
    logger.info(f"Organizer listening on '{bind_addr}'")

    await server.wait_closed()
    logger.info("Organizer stopped listening.")


# -------------------------------------------------------------------------------------------------

class OrganizerInbound(org.OrganizerInboundBase):
    srv: org.OrganizerOutboundStub  # connection back to Clapshot server
    log: Logger

    def __init__(self, logger, debug):
        self.log = logger
        self.debug = debug


    @override
    async def handshake(self, server_info: org.ServerInfo) -> clap.Empty:
        """
        Receive handshake from Clapshot server.
        We must connect back to it and send hanshake to establish a bidirectional connection.
        """
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
            await self.srv.handshake(org.OrganizerInfo(
                version=org.SemanticVersionNumber(major=int(VERSION.split(".")[0]), minor=int(VERSION.split(".")[1]), patch=int(VERSION.split(".")[2])),
                name=MODULE_NAME,
                description="Basic folders for the UI",
                hard_dependencies=[
                    org.OrganizerDependency(
                        name="clapshot.server",
                        min_ver=org.SemanticVersionNumber(major=0, minor=5, patch=6))
                ],
            ))
            self.log.info("Clapshot server connected.")

        except ConnectionRefusedError as e:
            self.log.error(f"Return connection to Clapshot server refused: {e}")
            raise GRPCError(GrpcStatus.UNKNOWN, "Failed to connect back to you (the Clapshot server)")

        # Open the database
        assert server_info.db.type == org.ServerInfoDatabaseDatabaseType.SQLITE, "Only SQLite is supported."
        self.log.info(f"Opening SQLite database at '{server_info.db.endpoint}'")
        self.db = sqlalchemy.create_engine(f"sqlite:///{server_info.db.endpoint}")   # , echo=self.debug, echo_pool=self.debug)
        self.DbNewSession = sessionmaker(bind=self.db)
        sqlalchemy.event.listen(self.db, 'connect', lambda c, _: c.execute('PRAGMA foreign_keys = ON;'))  # for every connection, enable foreign keys

        return clap.Empty()


    @override
    async def check_migrations(self, check_migrations_request: org.CheckMigrationsRequest) -> org.CheckMigrationsResponse:
        cur_ver, pending = db_check_pending_migrations(self.db)
        max_ver = sorted([m.version for m in pending], reverse=True)[0] if pending else cur_ver
        self.log.info(f"check_migrations(): current schema version = '{cur_ver}', max version = '{max_ver}', {len(pending)} pending migration alternatives")
        return org.CheckMigrationsResponse(current_schema_ver=cur_ver,pending_migrations=pending)


    @override
    async def apply_migration(self, apply_migration_request: org.ApplyMigrationRequest) -> org.ApplyMigrationResponse:
        self.log.info(f"apply_migration('{apply_migration_request.uuid}')")
        with self.DbNewSession() as dbs:
            db_apply_migration(dbs, apply_migration_request.uuid)
            return org.ApplyMigrationResponse()


    @override
    async def after_migrations(self, _: org.AfterMigrationsRequest) -> clap.Empty:
        log = self.log.getChild("after_migration")
        with self.DbNewSession() as dbs:
            db_test_orm_mappings(dbs, log)
            db_check_and_fix_integrity(dbs, log)
            # TODO: Maybe check for folder loops etc. here, too?
            return clap.Empty()


    @override
    async def on_start_user_session(self, req: org.OnStartUserSessionRequest) -> org.OnStartUserSessionResponse:
        """
        New user session started. Send the clien a list of actions that this organizer plugin supports.
        """
        self.log.info("on_start_user_session")
        assert req.ses.sid, "No session ID"
        await self.srv.client_define_actions(org.ClientDefineActionsRequest(
            sid = req.ses.sid,
            actions = self._make_custom_actions_map()))

        return org.OnStartUserSessionResponse()


    @override
    async def navigate_page(self, navigate_page_request: org.NavigatePageRequest) -> org.ClientShowPageRequest:
        ses = navigate_page_request.ses
        return await self._construct_navi_page(ses, None)


    @override
    async def cmd_from_client(self, cmd: org.CmdFromClientRequest) -> clap.Empty:
        if cmd.cmd == "new_folder":
            args = json.loads(cmd.args)
            parent_folder = (await self._get_current_folder_path(cmd.ses, None))[-1]
            with self.DbNewSession() as dbs:
                try:
                    # Create folder & refresh user's view
                    args = json.loads(cmd.args)
                    if new_folder_name := args.get("name"):
                        new_fld = await self._create_folder(dbs, cmd.ses, parent_folder, new_folder_name)
                        self.log.debug(f"Folder {new_fld.id} ('{new_fld.title}') created & committed, refreshing client's page")
                        navi_page = await self._construct_navi_page(cmd.ses, None)
                        await self.srv.client_show_page(navi_page)
                    else:
                        self.log.error("new_folder command missing 'name' argument")
                        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "new_folder command missing 'name' argument")
                except Exception as e:
                    self.log.error(f"Failed to create folder: {e}")
                    raise GRPCError(GrpcStatus.INTERNAL, "Failed to create folder")

        elif cmd.cmd == "open_folder":
            # Validate & parse argument JSON
            open_args = json.loads(cmd.args)
            assert isinstance(open_args, dict), "open_folder argument not a dict"
            folder_id = open_args.get("id")
            assert folder_id, "open_folder arg 'id' missing"
            assert isinstance(folder_id, int), "open_folder arg 'id' not an int"

            # Construct new breadcrumb trail
            trail = [f.id for f in (await self._get_current_folder_path(cmd.ses, None))]
            if folder_id in trail:
                trail = trail[:trail.index(folder_id)+1] # go up in current trail => remove all after this folder
            else:
                trail.append(folder_id) # add folder id at the end

            # Update folder path cookie
            new_cookie = json.dumps(trail)
            self.log.debug(f"Setting new folder_path cookie: {new_cookie}")
            await self.srv.client_set_cookies(org.ClientSetCookiesRequest(
                cookies = {PATH_COOKIE_NAME: new_cookie},
                sid = cmd.ses.sid))

            # Update page to view the opened folder
            page = await self._construct_navi_page(cmd.ses, new_cookie)
            await self.srv.client_show_page(page)

        elif cmd.cmd == "rename_folder":
            args = json.loads(cmd.args)  # {"id": 123, "new_name": "New name"}
            if not args or not args.get("id") or not args.get("new_name"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "rename_folder command missing 'id' or 'new_name' argument")
            folder_id = int(args["id"])
            with self.DbNewSession() as dbs:
                with dbs.begin_nested():
                    fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
                    if not fld:
                        raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{args['id']}' not found")
                    if fld.user_id not in (cmd.ses.user.id,"admin"):
                        raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot rename another user's folder")
                    fld.title = args["new_name"]

            self.log.debug(f"Renamed folder '{fld.id}' to '{fld.title}'")
            page = await self._construct_navi_page(cmd.ses, None)
            await self.srv.client_show_page(page)

        elif cmd.cmd == "trash_folder":
            args = json.loads(cmd.args) # {"id": 123}
            if not args or not args.get("id"):
                raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "trash_folder command missing 'id' argument")
            folder_id = int(args["id"])

            # Delete the folder and its contents, gather video IDs to delete later (after transaction, to avoid DB locks)
            videos_to_delete = []
            with self.DbNewSession() as dbs:
                with dbs.begin_nested():
                    videos_to_delete = await self._trash_folder_recursive(dbs, folder_id, cmd.ses.user.id)

            # Trash the videos
            for vi in videos_to_delete:
                self.log.debug(f"Trashing video '{vi}'")
                await self.srv.delete_video(org.DeleteVideoRequest(id=vi))  # this cleans up the video's files, too


            page = await self._construct_navi_page(cmd.ses, None)
            await self.srv.client_show_page(page)
        else:
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, f"Unknown organizer command: {cmd.cmd}")

        return clap.Empty()


    @override
    async def move_to_folder(self, req: org.MoveToFolderRequest) -> clap.Empty:
        is_admin = req.ses.user.id == "admin"   # admin can move anything anywhere, and transfer ownership in the process
        if not req.ids:
            self.log.warning("move_to_folder called with empty list of items. Bug in client?")
            return clap.Empty()

        with self.DbNewSession() as dbs:
            with dbs.begin_nested():
                dst_folder = dbs.query(DbFolder).filter(DbFolder.id == int(req.dst_folder_id)).one_or_none()
                max_sort_order = dbs.query(sqlalchemy.func.max(DbFolderItems.sort_order)).filter(DbFolderItems.folder_id == int(req.dst_folder_id)).scalar() or 0

                if not dst_folder:
                    raise GRPCError(GrpcStatus.NOT_FOUND, "Destination folder not found")
                if dst_folder.user_id != req.ses.user.id and not is_admin:
                    raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot move items to another user's folder")

                for it in req.ids:
                    # Move a folder
                    if it.folder_id:
                        fld_to_move = dbs.query(DbFolder).filter(DbFolder.id == int(it.folder_id)).one_or_none()

                        if not fld_to_move:
                            raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder id '{it.folder_id}' not found")
                        if fld_to_move.id == dst_folder.id:
                            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Cannot move a folder into itself")
                        if fld_to_move.user_id != req.ses.user.id and not is_admin:
                            raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot move another user's folder")

                        fld_to_move.user_id = dst_folder.user_id  # transfer ownership
                        cnt = dbs.query(DbFolderItems).filter(DbFolderItems.subfolder_id == fld_to_move.id).update({"folder_id": dst_folder.id, "sort_order": max_sort_order+1})
                        if cnt == 0:
                            raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder with ID '{fld_to_move.id}' is a root folder? Cannot move.")
                        else:
                            self.log.debug(f"Moved folder '{fld_to_move.id}' to folder '{dst_folder.id}'")

                    # Move a video
                    elif it.video_id:
                        vid_to_move = dbs.query(DbVideo).filter(DbVideo.id == it.video_id).one_or_none()

                        if not vid_to_move:
                            raise GRPCError(GrpcStatus.NOT_FOUND, f"Video '{it.video_id}' not found")
                        if vid_to_move.user_id != req.ses.user.id and not is_admin:
                            raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot move another user's video")

                        vid_to_move.user_id = dst_folder.user_id  # transfer ownership
                        cnt = dbs.query(DbFolderItems).filter(DbFolderItems.video_id == vid_to_move.id).update({"folder_id": dst_folder.id, "sort_order": max_sort_order+1})
                        if cnt == 0:  # not in any folder yet => insert it
                            dbs.add(DbFolderItems(folder_id=dst_folder.id, video_id=vid_to_move.id, sort_order=max_sort_order+1))
                        else:
                            self.log.debug(f"Moved video '{vid_to_move.id}' to folder '{dst_folder.id}'")

        # Update page to view the opened folder (after transaction commit!)
        page = await self._construct_navi_page(req.ses, None)
        await self.srv.client_show_page(page)
        return clap.Empty()


    @override
    async def reorder_items(self, req: org.ReorderItemsRequest) -> clap.Empty:
        is_admin = req.ses.user.id == "admin"
        if not req.ids:
            self.log.warning("reorder_items called with empty list of items. Bug in client?")
            return clap.Empty()

        if parent_folder_id := req.listing_data.get("folder_id"):
            with self.DbNewSession() as dbs:
                with dbs.begin_nested():

                    # Check destination folder
                    parent_folder = dbs.query(DbFolder).filter(DbFolder.id == int(parent_folder_id)).one_or_none()
                    if not parent_folder:
                        raise GRPCError(GrpcStatus.NOT_FOUND, f"Parent folder {parent_folder_id} not found")
                    if parent_folder.user_id != req.ses.user.id and not is_admin:
                        raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot reorder items in another user's folder")

                    # Reorder items
                    for i, it in enumerate(req.ids):
                        if it.folder_id:
                            cnt = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == parent_folder.id, DbFolderItems.subfolder_id == int(it.folder_id)).update({"sort_order": i})
                            if cnt == 0:
                                self.log.warning(f"DB inconsistency? Folder ID '{it.folder_id}' not in folder '{parent_folder.id}. Reordering skipped'")
                        elif it.video_id:
                            cnt = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == parent_folder.id, DbFolderItems.video_id == it.video_id).update({"sort_order": i})
                            if cnt == 0:
                                self.log.warning(f"DB inconsistency? Video ID '{it.video_id}' not in folder '{parent_folder.id}. Reordering skipped'")

                    return clap.Empty()
        else:
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "No folder ID in UI listing, cannot reorder")


    @override
    async def authz_user_action(
        self, authz_user_action_request: org.AuthzUserActionRequest) -> org.AuthzResponse:
        raise GRPCError(GrpcStatus.UNIMPLEMENTED)   # = let Clapshot server decide

    # -------------------------------------------------------------------------------------------------

    @override
    async def list_tests(self, clapshot_empty: clap.Empty) -> org.ListTestsResponse:
        self.log.info("list_tests")
        return org.ListTestsResponse(test_names=[])


    @override
    async def run_test(self, run_test_request: org.RunTestRequest) -> org.RunTestResponse:
        self.log.info("run_test")
        raise GRPCError(GrpcStatus.UNIMPLEMENTED)


    # -------------------------------------------------------------------------------------------------


    async def _construct_navi_page(self, ses: org.UserSessionData, cookie_override: Optional[str]=None) -> org.ClientShowPageRequest:
        """
        Construct the main navigation page for given user session.
        """
        folder_path = await self._get_current_folder_path(ses, cookie_override)
        assert len(folder_path)>0, "Folder path should always contain at least the root folder"
        cur_folder = folder_path[-1]

        folder_db_items = await self._fetch_folder_contents(cur_folder, ses.user.id)

        popup_actions = ["popup_builtin_rename", "popup_builtin_trash"]
        listing_data = {"folder_id": str(cur_folder.id)}

        if len(folder_path) > 1:
            # If not in root folder, add "move to parent" action to all items
            popup_actions.append("move_to_parent")
            listing_data["parent_folder_id"] = str(folder_path[-2].id)


        video_ids = [v.id for v in folder_db_items if isinstance(v, DbVideo)]
        video_list = await self.srv.db_get_videos(org.DbGetVideosRequest(ids = org.IdList(ids=video_ids)))
        videos_by_id = {v.id: v for v in video_list.items}

        async def _video_to_page_item(vid_id: str, popup_actions: list[str]) -> clap.PageItemFolderListingItem:
            assert re.match(r"^[0-9a-f]+$", vid_id), f"Unexpected video ID format: {vid_id}"
            return clap.PageItemFolderListingItem(
                video = videos_by_id[vid_id],
                open_action = clap.ScriptCall(
                    lang = clap.ScriptCallLang.JAVASCRIPT,
                    code = f'clapshot.openVideo("{vid_id}");'),
                popup_actions = popup_actions,
                vis = None)

        listing_items: list[clap.PageItemFolderListingItem] = []
        for itm in folder_db_items:
            if isinstance(itm, DbFolder):
                listing_items.append(await self._folder_to_page_item(itm, popup_actions, ses.user.id))
            elif isinstance(itm, DbVideo):
                listing_items.append(await _video_to_page_item(itm.id, popup_actions))
            else:
                raise ValueError(f"Unknown item type: {itm}")

        folder_listing = clap.PageItemFolderListing(
            items = listing_items,
            allow_reordering = True,
            popup_actions = ["new_folder"],
            listing_data = listing_data,
            allow_upload=True,
            video_added_action = "on_video_added")

        def _make_breadcrumbs_html(folder_path: list[DbFolder]) -> Optional[str]:
            if not folder_path:
                return None

            breadcrumbs: list[tuple[int, str]] = [(f.id, str(f.title or "UNNAMED")) for f in folder_path]
            breadcrumbs[0] = (breadcrumbs[0][0], "[Home]")    # rename root folder to "Home" for UI
            breadcrumbs_html =[]

            # Link all but last item
            for (id, title) in breadcrumbs[:-1]:
                args_json = json.dumps({'id': id}).replace('"', "'")
                title = html_escape(title)
                breadcrumbs_html.append(f'<a style="text-decoration: underline;" href="javascript:clapshot.callOrganizer(\'open_folder\', {args_json});">{title}</a>')
            # Last item in bold
            for (_, title) in breadcrumbs[-1:]:
                breadcrumbs_html.append(f"<strong>{html_escape(title)}</strong>")

            return " ▶ ".join(breadcrumbs_html) if len(breadcrumbs) > 1 else None

        pg_items = [ clap.PageItem(folder_listing=folder_listing) ]
        if html := _make_breadcrumbs_html(folder_path):
            pg_items.insert(0, clap.PageItem(html=html))   # add to first pos

        return org.ClientShowPageRequest(sid = ses.sid, page_items = pg_items)



    async def _get_current_folder_path(self, ses: org.UserSessionData, cookie_override: Optional[str]) -> list[DbFolder]:
        """
        Get current folder path from cookies & DB.

        If the cookie is malformed, it will be replaced with an empty one.
        Returned list will always contain at least one item (root folder).

        If cookie_override is set, it will be used instead of the cookie from session.
        """
        res: list[DbFolder] = []
        ck = ses.cookies or {}
        with self.DbNewSession() as dbs:
            try:
                if folder_ids := json.loads((cookie_override or ck.get(PATH_COOKIE_NAME)) or '[]'):
                    assert all(isinstance(i, int) for i in folder_ids), "Folder path cookie contains non-integer IDs"

                    folders_unordered = dbs.query(DbFolder).filter(DbFolder.id.in_(folder_ids)).all()
                    if len(folders_unordered) == len(folder_ids):
                        # Reorder the folders to match the order in the cookie, and return
                        folders_by_id = {f.id: f for f in folders_unordered}
                        return [folders_by_id[id] for id in folder_ids if id in folders_by_id]
                    else:
                        self.log.warning("Some unknown folder IDs in folder_path cookie. Clearing it.")
                        await self.srv.client_set_cookies(org.ClientSetCookiesRequest(cookies = {PATH_COOKIE_NAME: ''}, sid = ses.sid))
                        await self.srv.client_show_user_message(org.ClientShowUserMessageRequest(
                            sid = ses.sid,
                            msg = clap.UserMessage(
                                message = "Some unknown folder IDs in folder_path cookie. Clearing it.",
                                user_id = ses.user.id,
                                type = clap.UserMessageType.ERROR)))

                self.log.debug("No folder_path cookie found. Returning empty folder path.")
                res = []
            except json.JSONDecodeError as e:
                self.log.error(f"Failed to parse folder_path cookie: {e}. Falling back to empty folder path.")
                res = []

            # Make sure root folder is always in the path
            user_root = await db_get_or_create_user_root_folder(dbs, ses, self.srv, self.log)
            if not res or res[0].id != user_root.id:
                res.insert(0, user_root)

        return res


    async def _fetch_folder_contents(self, folder: DbFolder, user_id: str) -> list[DbVideo | DbFolder]:
        """
        Fetch the contents of a folder from the database, sorted by the order in the folder.
        """
        if folder.user_id != user_id and user_id != "admin":
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot fetch contents of another user's folder")

        with self.DbNewSession() as dbs:
            folder_items = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder.id).order_by(DbFolderItems.sort_order,).all()

            # Get DbFolder and DbVideo objects for all folder items
            subfolder_ids = [fi.subfolder_id for fi in folder_items]
            subfolder_items = dbs.query(DbFolder).filter(DbFolder.id.in_(subfolder_ids)).all()
            subfolders_by_id = {f.id: f for f in subfolder_items}

            video_ids = [fi.video_id for fi in folder_items]
            video_items = dbs.query(DbVideo).filter(DbVideo.id.in_(video_ids)).all()
            videos_by_id = {v.id: v for v in video_items}

            # Replace folder item IDs with actual objects
            def _get_item(fi: DbFolderItems) -> DbVideo | DbFolder:
                if fi.video_id:
                    return videos_by_id[fi.video_id]
                elif fi.subfolder_id:
                    return subfolders_by_id[fi.subfolder_id]
                else:
                    raise ValueError("Folder item has neither video nor subfolder ID")
            res = [_get_item(fi) for fi in folder_items]

            # If all items have sort_order 0 (default) sort them by type and title
            if all(fi.sort_order == 0 for fi in folder_items):
                sorted_folders = sorted(subfolder_items, key=lambda f: f.title or str(f.id))
                sorted_videos = sorted(video_items, key=lambda v: v.title or str(v.id))
                res = sorted_folders + sorted_videos

            return res


    async def _trash_folder_recursive(self, dbs: Session, folder_id: int, user_id: str) -> list[str]:
        """
        Trash a folder and unbind its contents recursively.
        Returns a list of all video IDs that are to be deleted.
        """
        fld = dbs.query(DbFolder).filter(DbFolder.id == folder_id).one_or_none()
        if not fld:
            raise GRPCError(GrpcStatus.NOT_FOUND, f"Folder ID '{folder_id}' not found")
        if fld.user_id != user_id and user_id != "admin":
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, f"Cannot trash another user's folder")

        folder_items = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder_id).all()
        video_ids = [it.video_id for it in folder_items if it.video_id]

        self.log.debug(f"Deleting folder '{folder_id}' ('{fld.title}') and its contents")

        # Recurse to subfolders
        for fi in [it.subfolder_id for it in folder_items if it.subfolder_id]:
            video_ids.extend(await self._trash_folder_recursive(dbs, fi, user_id))

        # Remove content bindings
        dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == folder_id).delete()

        # Delete the folder itself
        dbs.query(DbFolder).filter(DbFolder.id == folder_id).delete()
        return video_ids


    async def _folder_to_page_item(self, fld: DbFolder, popup_actions: list[str], user_id: str) -> clap.PageItemFolderListingItem:
        """
        Convert a folder node to a page item.
        """
        prevw_items = await self._preview_items_for_folder(fld, user_id)

        return clap.PageItemFolderListingItem(
            folder = clap.PageItemFolderListingFolder(
                id = str(fld.id),
                title = fld.title or "<UNNAMED>",
                preview_items = prevw_items),
            open_action = clap.ScriptCall(
                lang = clap.ScriptCallLang.JAVASCRIPT,
                code = f'clapshot.callOrganizer("open_folder", {{id: {fld.id}}});'),
            popup_actions = popup_actions)


    async def _preview_items_for_folder(self, fld: DbFolder, user_id: str) -> list[clap.PageItemFolderListingItem]:
        contained_items = await self._fetch_folder_contents(fld, user_id)

        contained_videos = [itm for itm in contained_items if isinstance(itm, DbVideo)][:4] # Client UI currently only shows max 4 items, don't bother with more
        video_objs: org.DbVideoList = await self.srv.db_get_videos(org.DbGetVideosRequest(ids = org.IdList(ids=[v.id for v in contained_videos])))
        videos_by_id = {v.id: v for v in video_objs.items}

        res = []
        for itm in contained_items:
            if isinstance(itm, DbFolder):
                res.append(clap.PageItemFolderListingItem(
                    folder = clap.PageItemFolderListingFolder(id = str(itm.id), title = itm.title or "<UNNAMED>")))
            elif isinstance(itm, DbVideo):
                res.append(clap.PageItemFolderListingItem(video = videos_by_id[itm.id]))
            else:
                raise ValueError(f"Unknown item type: {itm}")
        return res


    async def _create_folder(self, dbs: Session, ses: org.UserSessionData, parent_folder: DbFolder, new_folder_name: str) -> DbFolder:
        assert parent_folder is not None, "Cannot create root folders with this function"
        is_admin = ses.user.id == "admin"

        if parent_folder.user_id != ses.user.id and not is_admin:
            raise GRPCError(GrpcStatus.PERMISSION_DENIED, "Cannot create folder in another user's folder")
        if len(new_folder_name) > 255:
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Folder name too long")
        if not new_folder_name:
            GRPCError(GrpcStatus.INVALID_ARGUMENT, "Folder name cannot be empty")

        if new_folder_name in [f.title for f in await self._fetch_folder_contents(parent_folder, ses.user.id)]:
            raise GRPCError(GrpcStatus.ALREADY_EXISTS, "Item with this name already exists in the this folder")

        with dbs.begin_nested():
            # Create the new folder
            new_folder = DbFolder(user_id=parent_folder.user_id, title=new_folder_name)
            dbs.add(new_folder)
            dbs.flush()

            # Add it at the end of the parent folder
            max_sort_order = dbs.query(sqlalchemy.func.max(DbFolderItems.sort_order)).filter(DbFolderItems.folder_id == parent_folder.id).scalar() or 0
            dbs.add(DbFolderItems(folder_id=parent_folder.id, subfolder_id=new_folder.id, sort_order=max_sort_order+1))
            return new_folder


    def _make_custom_actions_map(self) -> dict[str, clap.ActionDef]:
        """
        Popup actions for when the user right-clicks on a listing background.
        """
        def _make_new_folder_action() -> clap.ActionDef:
            return clap.ActionDef(
                ui_props = clap.ActionUiProps(
                    label = "New folder",
                    icon = clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-folder-plus", color=None)),
                    key_shortcut = None,
                    natural_desc = "Create a new folder"),
                action = clap.ScriptCall(
                    lang = clap.ScriptCallLang.JAVASCRIPT,
                    code = dedent("""
                        var folder_name = (prompt("Name for the new folder", ""))?.trim();
                        if (folder_name) { clapshot.callOrganizer("new_folder", {name: folder_name}); }
                    """).strip()))

        def _make_move_to_parent_action() -> clap.ActionDef:
            return clap.ActionDef(
                ui_props = clap.ActionUiProps(
                    label = "Move to parent",
                    icon = clap.Icon(fa_class=clap.IconFaClass(classes="fa fa-arrow-turn-up", color=None)),
                    key_shortcut = None,
                    natural_desc = "Move item to parent folder"),
                action = clap.ScriptCall(
                    lang = clap.ScriptCallLang.JAVASCRIPT,
                    code = dedent("""
                        var listingData = _action_args.listing_data;
                        var items = _action_args.selected_items;

                        if (!listingData.parent_folder_id) {
                            alert("parent_folder_id missing from listingData.");
                            return;
                        }
                        var folderId = listingData.parent_folder_id;
                        var ids = clapshot.itemsToIDs(items);
                        clapshot.moveToFolder(folderId, ids, listingData);
                    """).strip()))

        def _make_on_video_added_action() -> clap.ActionDef:
            return clap.ActionDef(
                ui_props = None,    # not an UI action, just a callback
                action = clap.ScriptCall(
                    lang = clap.ScriptCallLang.JAVASCRIPT,
                    code = dedent("""
                        var vid = _action_args.video_id;
                        var listingData = _action_args.listing_data;
                        var folderId = listingData?.folder_id;

                        if (!folderId || !vid) {
                            var msg = "on_video_added error: video_id missing, or folder_id from listingData.";
                            alert(msg); console.error(msg);
                        } else {
                            clapshot.moveToFolder(folderId, [{videoId: vid}], listingData);
                        }
                    """).strip()))

        return {
            "new_folder": _make_new_folder_action(),
            "move_to_parent": _make_move_to_parent_action(),
            "on_video_added": _make_on_video_added_action(),
        }



if __name__ == '__main__':
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("EXIT signaled.")
