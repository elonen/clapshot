from __future__ import annotations

import asyncio
import json
import sys, inspect
from contextlib import redirect_stdout, redirect_stderr
from io import StringIO
import traceback
from types import MethodType
from typing import Tuple

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus
import clapshot_grpc.clapshot.organizer as org
import clapshot_grpc.clapshot as clap

import organizer
from organizer.config import PATH_COOKIE_NAME
from organizer.database.models import DbFolder, DbUser, DbVideo
from organizer.database.operations import db_get_or_create_user_root_folder
from organizer.helpers.folders import FoldersHelper


async def list_tests_impl(oi: organizer.OrganizerInbound) -> org.ListTestsResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server to list all available unit/integrations tests in this plugin.
    => Uses `inspection` to find all functions in this module that start with 'org_test_'.
    """
    oi.log.info("list_tests")
    current_module = sys.modules[__name__]
    test_names = sorted([
        func_name for func_name, func in inspect.getmembers(current_module, inspect.isfunction)
        if func_name.startswith('org_test_')
    ])
    return org.ListTestsResponse(test_names=test_names)


async def run_test_impl(oi, request: org.RunTestRequest) -> org.RunTestResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server to run a single unit/integration test in this plugin, by name, no arguments.
    => Uses `inspection` to find the test function by name and execute it with `redirect_stdout` to capture output.
    """
    oi.log.info(f"Running test: {request.test_name}")
    test_name = request.test_name
    current_module = sys.modules[__name__]

    # Attempt to get the test function by name
    test_func = getattr(current_module, test_name, None)
    if test_func is None or not test_name.startswith('org_test_'):
        oi.log.error(f"Test function {test_name} not found or is invalid.")
        return org.RunTestResponse(output="", error=f"Test function {test_name} not found or is invalid.")

    # Set up StringIO stream to capture output
    buffer = StringIO()
    try:
        with redirect_stdout(buffer), redirect_stderr(buffer):
            # Execute the test function
            if asyncio.iscoroutinefunction(test_func):
                result = await test_func(oi)
            else:
                result = test_func(oi)
            # Capture additional return value if needed
            output = buffer.getvalue()
            if result is not None:
                output += f"\nReturn: {result}"
    except Exception as e:
        oi.log.error(f"Error running {test_name}: {str(e)}")
        error_with_traceback = "".join(traceback.format_exception(type(e), e, e.__traceback__))
        return org.RunTestResponse(output=buffer.getvalue(), error=str(error_with_traceback))

    # Successful execution with output captured
    return org.RunTestResponse(output=output, error=None)


# ---------------------------- Test functions --------------------------------
# These are the test functions that the server will call when running tests.
#
# At that point, handshake and database migrations have already been done,
# so these tests can assume that the temporary test database is ready for use.
# ----------------------------------------------------------------------------

# TODO: Refactor @overridden OrganizerInbound methods into smaller parts that _return_ any client messages insted of sending them directly, and assert them in the test functions.


async def org_test__start_user_session(oi: organizer.OrganizerInbound):
    """
    on_start_user_session() -- Just a simple test to check if the method doesn't crash.
    """
    user = oi.db_new_session().query(DbUser).first()
    res = await organizer.on_start_user_session_impl(oi, org.OnStartUserSessionRequest(
        org.UserSessionData(
            sid="test_sid",user=clap.UserInfo(id=user.id, name=user.name),
            is_admin=False, cookies={})
    ))
    assert res == org.OnStartUserSessionResponse()


async def org_test__navigate_page(oi: organizer.OrganizerInbound):
    """
    navigate_page() -- Test that it returns a valid ClientShowPageRequest.
    """
    user = oi.db_new_session().query(DbUser).first()
    res = await organizer.navigate_page_impl(oi, org.NavigatePageRequest(
        ses=org.UserSessionData(sid="test_sid",user=clap.UserInfo(id=user.id, name=user.name), cookies={})
    ))
    assert isinstance(res, org.ClientShowPageRequest)
    assert len(res.page_items) > 0


async def org_test__authz_user_action(oi: organizer.OrganizerInbound):
    """
    authz_user_action() -- Should return UNIMPLEMENTED.
    """
    try:
        await oi.authz_user_action(org.AuthzUserActionRequest())
        assert False, "Expected GRPCError"
    except GRPCError as e:
        assert e.status == GrpcStatus.UNIMPLEMENTED


async def org_test__move_to_folder(oi: organizer.OrganizerInbound):
    """
    move_to_folder() -- Test several scenarios of moving folders and videos between folders.
    """
    videos = await oi.srv.db_get_videos(org.DbGetVideosRequest(all=clap.Empty()))
    assert len(videos.items) > 0, "No videos found in the test database"

    users_video = videos.items[0]
    (user_id, user_name) = (users_video.user_id, f"Name of {users_video.user_id}")

    ses = org.UserSessionData(sid="test_sid",user=clap.UserInfo(id=user_id, name=user_name), is_admin=False, cookies={})

    # Get folder path. This should actually create the root folder, since the test database is empty.
    fld_path = await oi.folders_helper.get_current_folder_path(ses)
    assert len(fld_path) > 0, "Folder path should always contain at least the root folder"
    root_fld = fld_path[0]

    # Organizer should have moved all orphan videos to the root folder, so the user's video should be there now
    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    assert any(v.id == users_video.id for v in root_cont), "Video should have been auto-moved to the root folder"

    # 1) First, try to move someone else's video to the root folder (should fail)
    someone_elses_video = [v for v in videos.items if v.user_id != user_id][0]
    assert someone_elses_video.user_id
    oi.log.info(f"Trying to move someone else's ({someone_elses_video.user_id}) video ({someone_elses_video.id}) to the root folder ({root_fld.id}) of current user ({user_id})")
    try:
        await organizer.move_to_folder_impl(oi, org.MoveToFolderRequest(
            ses,
            ids=[clap.FolderItemId(video_id=someone_elses_video.id)],
            dst_folder_id=str(root_fld.id),
            listing_data={}))
        assert False, "Expected GRPCError (permission denied)"
    except GRPCError as e:
        assert e.status == GrpcStatus.PERMISSION_DENIED

    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    assert any(v.id == users_video.id for v in root_cont), "Video should still be in the root folder"

    # 2) Next, create a new subfolder and move the video there. This should succeed.
    subfld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Subfolder")

    oi.log.info(f"Moving user's ({user_id}) video ({users_video.id}) to the subfolder ({subfld.id})")
    await oi.move_to_folder(org.MoveToFolderRequest(
        ses,
        ids=[clap.FolderItemId(video_id=users_video.id)],
        dst_folder_id=str(subfld.id),
        listing_data={}))

    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    assert not any(v.id == users_video.id for v in root_cont), "Video not be in the root folder anymore"
    subfld_cont = await oi.folders_helper.fetch_folder_contents(subfld, ses)
    assert any(v.id == users_video.id for v in subfld_cont), "Video should be in the subfolder now"



async def org_test__reorder_items(oi: organizer.OrganizerInbound):
    """
    reorder_items() -- Test reordering of folders and videos within a folder in various ways.
    """
    # Fetch a video from the test database, and get the user ID
    videos = await oi.srv.db_get_videos(org.DbGetVideosRequest(all=clap.Empty()))
    assert len(videos.items) > 0, "No videos found in the test database"
    users_video = videos.items[0]
    (user_id, user_name) = (users_video.user_id, f"Name of {users_video.user_id}")

    ses = org.UserSessionData(sid="test_sid",user=clap.UserInfo(id=user_id, name=user_name), is_admin=False, cookies={})

    # Get folder path (+ create root folder + move orphan videos to root folder)
    fld_path = await oi.folders_helper.get_current_folder_path(ses)
    assert len(fld_path) > 0, "Folder path should always contain at least the root folder"
    root_fld = fld_path[0]

    # Create two folders for testing in the root
    subfld1 = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Subfolder 1")
    subfld2 = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Subfolder 2")

    # Move any other videos to subfld1, so they don't interfere with the reorder test.
    other_videos = [v for v in videos.items if v.id != users_video.id and v.user_id == user_id]
    for v in other_videos:
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses,
            ids=[clap.FolderItemId(video_id=v.id)],
            dst_folder_id=str(subfld1.id),
            listing_data={}))

    # Also create a third folder, but this time inside subfld2. This shouldn't affect the reorder test.
    await oi.folders_helper.create_folder(oi.db_new_session(), ses, subfld2, "Test Subsubfolder")

    test_orders: list[list[DbFolder | clap.Video]] = [
        [subfld1, subfld2, users_video],
        [users_video, subfld2, subfld1],
        [subfld2, users_video, subfld1],
    ]
    for i, new_obj_order in enumerate(test_orders):
        new_order = [clap.FolderItemId(folder_id=str(fi.id))
                     if isinstance(fi, DbFolder)
                     else clap.FolderItemId(video_id=fi.id) for fi in new_obj_order]
        await oi.reorder_items(org.ReorderItemsRequest(ses, ids=new_order, listing_data={"folder_id": str(root_fld.id)}))
        cont = [fi.id for fi in await oi.folders_helper.fetch_folder_contents(root_fld, ses)]
        new_order_ids = [fi.id for fi in new_obj_order]
        print(f"Test #{i+1}", "Expecting:", new_order_ids, "Got:", cont)
        assert cont == new_order_ids, f"Wrong order after reorder #{i+1}"


async def _create_test_folder_and_session(oi: organizer.OrganizerInbound) -> Tuple[org.UserSessionData, DbFolder]:
    """
    Helper for the org_test__cmd_from_client__* tests.
    """
    with oi.db_new_session() as dbs:
        user_id, user_name = "cmdfromclient.test_user", "Cmdfromclient Test User"
        dbs.add(DbUser(id=user_id, name=user_name))
        dbs.commit()

    with oi.db_new_session() as dbs:
        ses = org.UserSessionData(sid="test_sid",user=clap.UserInfo(id=user_id, name=user_name), is_admin=False, cookies={})

        # Check that the user has no folders yet (including root folder)
        flds = dbs.query(DbFolder).filter(DbFolder.user_id == user_id).all()
        assert len(flds) == 0, "User should have no folders yet"

        # Get/create the root folder for the user
        flds = await oi.folders_helper.get_current_folder_path(ses)
        assert len(flds) == 1, "User should now have a root folder"
        root_fld = flds[0]

        return ses, root_fld


async def org_test__cmd_from_client__new_folder(oi: organizer.OrganizerInbound):
    """
    cmd_from_client() -- Test the 'new_folder' client command.
    """
    ses, root_fld = await _create_test_folder_and_session(oi)

    # Send the 'new_folder' client command
    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="new_folder",
        args='{"name": "Test Folder"}'))

    # Check that the new folder was created
    cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    print("Folder contents:", cont)
    flds = [fi for fi in cont if isinstance(fi, DbFolder)]
    assert len(flds) == 1, "Root folder should have one subfolder now"
    assert flds[0].title == "Test Folder"


async def org_test__cmd_from_client__open_folder(oi: organizer.OrganizerInbound):
    """
    cmd_from_client() -- Test that 'open_folder' client command sets the folder path cookie correctly.
    """
    orig_set_cookies = oi.srv.client_set_cookies
    try:
        ses, root_fld = await _create_test_folder_and_session(oi)
        new_fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Folder")
        expected_path = [root_fld.id, new_fld.id]

        # Mock the client_set_cookies method to check the cookie value
        async def mock_set_cookies(self, req: org.ClientSetCookiesRequest) -> clap.Empty:
            nonlocal ses
            ses.cookies = req.cookies
            assert req.cookies[PATH_COOKIE_NAME] == json.dumps(expected_path)
            return await orig_set_cookies(req)

        setattr(oi.srv, "client_set_cookies", MethodType(mock_set_cookies, oi.srv))

        # Send the 'open_folder' client command
        ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])
        await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
            cmd="open_folder",
            args=json.dumps({"id": new_fld.id})))

        # Check that the new folder was opened
        flds = await oi.folders_helper.get_current_folder_path(ses)
        path_got = [f.id for f in flds]
        print("Folder path that was set:", path_got, "Expected:", expected_path)
        assert path_got == expected_path, "Folder path should have been updated"

    finally:
        # Restore the original method
        setattr(oi.srv, "client_set_cookies", orig_set_cookies)


async def org_test__cmd_from_client__rename_folder(oi: organizer.OrganizerInbound):
    """
    cmd_from_client() -- Test the 'rename_folder' client command against database.
    """
    ses, root_fld = await _create_test_folder_and_session(oi)
    new_fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Folder")

    # Send the 'rename_folder' client command
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="rename_folder",
        args=json.dumps({"id": new_fld.id, "new_name": "Test Folder New Name"})))

    # Check that the folder was renamed
    cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    flds = [fi for fi in cont if isinstance(fi, DbFolder)]
    assert len(flds) == 1
    assert flds[0].title == "Test Folder New Name"


async def org_test__cmd_from_client__trash_folder(oi: organizer.OrganizerInbound):
    """
    cmd_from_client() -- Test the 'trash_folder' client command against database.
    """
    ses, root_fld = await _create_test_folder_and_session(oi)
    new_fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Folder")

    # Send the 'trash_folder' client command
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="trash_folder",
        args=json.dumps({"id": new_fld.id})))

    # Check that the folder was trashed
    cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    flds = [fi for fi in cont if isinstance(fi, DbFolder)]
    assert len(flds) == 0, "Folder should have been deleted"


async def org_test__admin_owner_transfer(oi: organizer.OrganizerInbound):
    """
    Test move_to_folder() as an admin -- Admin can move any folder or video to any user's folder.
       When moving a folder into another user's folder, ownership of the source folder and all its contents
       are transferred to the destination folder's owner.
    """
    # Fetch a video from the test database
    videos = await oi.srv.db_get_videos(org.DbGetVideosRequest(all=clap.Empty()))
    assert len(videos.items) > 0, "No videos found in the test database"

    video_owners = {v.id: v.user_id for v in videos.items}

    # Get user for the video
    src_video = videos.items[0]
    src_user = clap.UserInfo(id=src_video.user_id, name=f"Name of {src_video.user_id}")
    src_user_ses = org.UserSessionData(sid="test_sid", user=src_user, is_admin=False, cookies={})

    # Create a folder + a subfolder for the source user
    with oi.db_new_session() as dbs:
        src_root_fld = await db_get_or_create_user_root_folder(dbs, src_user, oi.srv, oi.log)
        src_fld = await oi.folders_helper.create_folder(dbs, src_user_ses, src_root_fld, "Ownertransfer Test Folder")
        src_subfld = await oi.folders_helper.create_folder(dbs, src_user_ses, src_fld, "Ownertransfer Test Subfolder")

    # Move the video to the subfolder
    await oi.move_to_folder(org.MoveToFolderRequest(
        src_user_ses,
        ids=[clap.FolderItemId(video_id=src_video.id)],
        dst_folder_id=str(src_subfld.id),
        listing_data={}))

    assert src_video.user_id == src_user.id
    assert src_fld.user_id == src_user.id
    assert src_subfld.user_id == src_user.id


    # Create a new user and session
    dst_user = clap.UserInfo(id="ownertransfer-test.dst_user", name="Ownertransfer Test User")
    with oi.db_new_session() as dbs:
        dbs.add(DbUser(id=dst_user.id, name=dst_user.name))
        dbs.commit()
    dst_user_ses = org.UserSessionData(sid="test_sid2",user=clap.UserInfo(id=dst_user.id, name=dst_user.name), is_admin=False, cookies={})

    # Create a folder for the destination user
    with oi.db_new_session() as dbs:
        dst_root_fld = await db_get_or_create_user_root_folder(dbs, dst_user, oi.srv, oi.log)
        dst_fld = await oi.folders_helper.create_folder(dbs, dst_user_ses, dst_root_fld, "Ownertransfer Destination Folder")

    assert dst_root_fld.user_id == dst_user.id
    assert dst_fld.user_id == dst_user.id

    # No ownership transfer should've happened yet, check that video_owners matches
    new_owners = await oi.srv.db_get_videos(org.DbGetVideosRequest(all=clap.Empty()))
    for v in new_owners.items:
        assert video_owners[v.id] == v.user_id

    # As an admin, move the source user's folder into the destination user's folder
    admin_ses = org.UserSessionData(sid="test_sid_admin", user=clap.UserInfo(id="test.admin", name="The Admin"), is_admin=True, cookies={})
    with oi.db_new_session() as dbs:
        dbs.add(DbUser(id=admin_ses.user.id, name=admin_ses.user.name))
        dbs.commit()

    await oi.move_to_folder(org.MoveToFolderRequest(
        admin_ses,
        ids=[clap.FolderItemId(folder_id=str(src_fld.id))],
        dst_folder_id=str(dst_fld.id),
        listing_data={}))

    # Check that ownership was transferred
    with oi.db_new_session() as dbs:
        src_fld = dbs.query(DbFolder).filter(DbFolder.id == src_fld.id).one_or_none()
        src_subfld = dbs.query(DbFolder).filter(DbFolder.id == src_subfld.id).one_or_none()
        src_video = dbs.query(DbVideo).filter(DbVideo.id == src_video.id).one_or_none()
        assert src_fld is not None
        assert src_subfld is not None
        assert src_video is not None
        assert src_fld.user_id == dst_user.id
        assert src_subfld.user_id == dst_user.id
        assert src_video.user_id == dst_user.id

    # Check that the folder hierarchy is intact, but in `dst_fld`.
    dst_fld_cont = await oi.folders_helper.fetch_folder_contents(dst_fld, dst_user_ses)
    assert any(fi.id == src_fld.id for fi in dst_fld_cont)
    src_fld_cont = await oi.folders_helper.fetch_folder_contents(src_fld, dst_user_ses)
    assert len(src_fld_cont) == 1
    assert src_fld_cont[0].id == src_subfld.id
    subfld_cont = await oi.folders_helper.fetch_folder_contents(src_subfld, dst_user_ses)
    assert len(subfld_cont) == 1
    assert subfld_cont[0].id == src_video.id

    # Check that the example video's ownership was transferred, but not other videos
    video_owners[src_video.id] = dst_user.id    # Update the expected owner
    new_owners = await oi.srv.db_get_videos(org.DbGetVideosRequest(all=clap.Empty()))
    for v in new_owners.items:
        assert video_owners[v.id] == v.user_id
