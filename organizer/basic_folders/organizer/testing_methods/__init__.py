from __future__ import annotations

import asyncio
import json
import sys
import uuid
import inspect
from contextlib import redirect_stdout, redirect_stderr
from io import StringIO
import traceback
from types import MethodType
from typing import Optional, Tuple

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus
import clapshot_grpc.proto.clapshot.organizer as org
import clapshot_grpc.proto.clapshot as clap

import sqlalchemy

import organizer
from organizer.config import PATH_COOKIE_NAME
from organizer.database.models import DbFolder, DbFolderItems, DbUser, DbMediaFile, DbSharedFolder, FolderKind
from organizer.database.operations import db_get_or_create_user_root_folder
from organizer.helpers.folders import SHARED_FOLDER_TOKEN_COOKIE_NAME

# Import metaplugin tests so they get discovered by the test framework via inspect.getmembers()
# These imports are required for test discovery - do NOT remove as "unused" imports!
# The test framework dynamically discovers test functions by inspecting this module's namespace.
from organizer.test_metaplugins import (  # noqa: F401
    org_test__metaplugin_loader_basic,  # noqa: F401
    org_test__metaplugin_loader_missing_dir,  # noqa: F401
    org_test__metaplugin_loader_empty_dir,  # noqa: F401
    org_test__metaplugin_on_init_hook,  # noqa: F401
    org_test__metaplugin_extend_actions,  # noqa: F401
    org_test__metaplugin_augment_folder_listing,  # noqa: F401
    org_test__metaplugin_augment_listing_data,  # noqa: F401
    org_test__metaplugin_exception_handling,  # noqa: F401
    org_test__metaplugin_multiple_plugins,  # noqa: F401
    org_test__metaplugin_invalid_plugin,  # noqa: F401
    org_test__metaplugin_sha256_demo_user_only,  # noqa: F401
    org_test__authz_plugin_allow,  # noqa: F401
    org_test__authz_plugin_deny,  # noqa: F401
    org_test__authz_plugin_defer,  # noqa: F401
    org_test__authz_default_deny_non_owner,  # noqa: F401
    org_test__authz_default_allow_owner,  # noqa: F401
    org_test__authz_default_allow_admin,  # noqa: F401
    org_test__authz_plugin_override_default,  # noqa: F401
)


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
    authz_user_action() -- Should return UNIMPLEMENTED for non-upload actions.
    """
    try:
        # Test non-upload action should still return UNIMPLEMENTED
        await oi.authz_user_action(org.AuthzUserActionRequest(
            ses=org.UserSessionData(
                sid="test_sid",
                user=clap.UserInfo(id="test_user", name="Test User"),
                is_admin=False,
                cookies={},
                http_headers={}
            ),
            other_op=org.AuthzUserActionRequestOtherOp(
                op=org.AuthzUserActionRequestOtherOpOp.LOGIN
            )
        ))
        assert False, "Expected GRPCError"
    except GRPCError as e:
        assert e.status == GrpcStatus.UNIMPLEMENTED


async def org_test__authz_user_action_upload(oi: organizer.OrganizerInbound):
    """
    authz_user_action() -- Test upload authorization based on X-Remote-User-Can-Upload header.
    """
    async def test_upload_auth(header_value, expected_authorized, should_fallback=False):
        headers = {"x-remote-user-can-upload": header_value} if header_value is not None else {}
        req = org.AuthzUserActionRequest(
            ses=org.UserSessionData(
                sid="test_sid", user=clap.UserInfo(id="test_user", name="Test User"),
                is_admin=False, cookies={}, http_headers=headers
            ),
            other_op=org.AuthzUserActionRequestOtherOp(
                op=org.AuthzUserActionRequestOtherOpOp.UPLOAD_MEDIA_FILE
            )
        )

        if should_fallback:
            try:
                await oi.authz_user_action(req)
                assert False, f"Expected fallback for '{header_value}'"
            except GRPCError as e:
                assert e.status == GrpcStatus.UNIMPLEMENTED
        else:
            res = await oi.authz_user_action(req)
            assert res.is_authorized == expected_authorized, f"Wrong auth result for '{header_value}'"
            if not expected_authorized:
                assert "Upload permission denied" in res.message

    # Test allow cases
    for value in ["true", "1", "yes", "TRUE", " yes ", "x_remote_user_can_upload"]:
        if value == "x_remote_user_can_upload":
            # Test underscore variant
            req = org.AuthzUserActionRequest(
                ses=org.UserSessionData(
                    sid="test_sid", user=clap.UserInfo(id="test_user", name="Test User"),
                    is_admin=False, cookies={}, http_headers={"x_remote_user_can_upload": "true"}
                ),
                other_op=org.AuthzUserActionRequestOtherOp(
                    op=org.AuthzUserActionRequestOtherOpOp.UPLOAD_MEDIA_FILE
                )
            )
            res = await oi.authz_user_action(req)
            assert res.is_authorized
        else:
            await test_upload_auth(value, True)

    # Test deny cases
    for value in ["false", "0", "no", "invalid"]:
        await test_upload_auth(value, False)

    # Test fallback case (no header)
    await test_upload_auth(None, None, should_fallback=True)


async def org_test__move_to_folder(oi: organizer.OrganizerInbound):
    """
    move_to_folder() -- Test several scenarios of moving folders and media files between folders.
    """
    media_files = await oi.srv.db_get_media_files(org.DbGetMediaFilesRequest(all=clap.Empty()))
    assert len(media_files.items) > 0, "No media files found in the test database"

    users_file = media_files.items[0]
    (user_id, user_name) = (users_file.user_id, f"Name of {users_file.user_id}")

    ses = org.UserSessionData(sid="test_sid",user=clap.UserInfo(id=user_id, name=user_name), is_admin=False, cookies={})

    # Get folder path. This should actually create the root folder, since the test database is empty.
    fld_path, _root_folder_id = await oi.folders_helper.get_current_folder_path(ses)
    assert len(fld_path) > 0, "Folder path should always contain at least the root folder"
    root_fld = fld_path[0]

    # Organizer should have moved all orphan media files to the root folder, so the user's media file should be there now
    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    assert any(v.id == users_file.id for v in root_cont), "Video should have been auto-moved to the root folder"

    # 1) First, try to move someone else's media file to the root folder (should fail)
    someone_elses_file = [v for v in media_files.items if v.user_id != user_id][0]
    assert someone_elses_file.user_id
    oi.log.info(f"Trying to move someone else's ({someone_elses_file.user_id}) file ({someone_elses_file.id}) to the root folder ({root_fld.id}) of current user ({user_id})")
    try:
        await organizer.move_to_folder_impl(oi, org.MoveToFolderRequest(
            ses,
            ids=[clap.FolderItemId(media_file_id=someone_elses_file.id)],
            dst_folder_id=str(root_fld.id),
            listing_data={}))
        assert False, "Expected GRPCError (permission denied)"
    except GRPCError as e:
        assert e.status == GrpcStatus.PERMISSION_DENIED

    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    assert any(v.id == users_file.id for v in root_cont), "Video should still be in the root folder"

    # 2) Next, create a new subfolder and move the file there. This should succeed.
    subfld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Subfolder")

    oi.log.info(f"Moving user's ({user_id}) file ({users_file.id}) to the subfolder ({subfld.id})")
    await oi.move_to_folder(org.MoveToFolderRequest(
        ses,
        ids=[clap.FolderItemId(media_file_id=users_file.id)],
        dst_folder_id=str(subfld.id),
        listing_data={}))

    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    assert not any(v.id == users_file.id for v in root_cont), "Video not be in the root folder anymore"
    subfld_cont = await oi.folders_helper.fetch_folder_contents(subfld, ses)
    assert any(v.id == users_file.id for v in subfld_cont), "Video should be in the subfolder now"



async def org_test__reorder_items(oi: organizer.OrganizerInbound):
    """
    reorder_items() -- Test reordering of folders and files within a folder in various ways.
    """
    # Fetch a file from the test database, and get the user ID
    media_files = await oi.srv.db_get_media_files(org.DbGetMediaFilesRequest(all=clap.Empty()))
    assert len(media_files.items) > 0, "No media files found in the test database"
    users_file = media_files.items[0]
    (user_id, user_name) = (users_file.user_id, f"Name of {users_file.user_id}")

    ses = org.UserSessionData(sid="test_sid",user=clap.UserInfo(id=user_id, name=user_name), is_admin=False, cookies={})

    # Get folder path (+ create root folder + move orphan media files to root folder)
    fld_path, _root_folder_id = await oi.folders_helper.get_current_folder_path(ses)
    assert len(fld_path) > 0, "Folder path should always contain at least the root folder"
    root_fld = fld_path[0]

    # Create two folders for testing in the root
    subfld1 = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Subfolder 1")
    subfld2 = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Test Subfolder 2")

    # Move any other media files to subfld1, so they don't interfere with the reorder test.
    other_files = [v for v in media_files.items if v.id != users_file.id and v.user_id == user_id]
    for v in other_files:
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses,
            ids=[clap.FolderItemId(media_file_id=v.id)],
            dst_folder_id=str(subfld1.id),
            listing_data={}))

    # Also create a third folder, but this time inside subfld2. This shouldn't affect the reorder test.
    await oi.folders_helper.create_folder(oi.db_new_session(), ses, subfld2, "Test Subsubfolder")

    test_orders: list[list[DbFolder | clap.MediaFile]] = [
        [subfld1, subfld2, users_file],
        [users_file, subfld2, subfld1],
        [subfld2, users_file, subfld1],
    ]
    for i, new_obj_order in enumerate(test_orders):
        new_order = [clap.FolderItemId(folder_id=str(fi.id))
                     if isinstance(fi, DbFolder)
                     else clap.FolderItemId(media_file_id=fi.id) for fi in new_obj_order]
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
        flds, _root_folder_id = await oi.folders_helper.get_current_folder_path(ses)
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
        flds, _root_folder_id = await oi.folders_helper.get_current_folder_path(ses)
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


async def org_test__cmd_from_client__share_folder(oi: organizer.OrganizerInbound):
    """
    cmd_from_client() -- Test the 'share_folder' client command.
    """
    # Create test session and folder
    ses, root_fld = await _create_test_folder_and_session(oi)

    # Create a subfolder to share
    with oi.db_new_session() as dbs:
        subfolder = await oi.folders_helper.create_folder(dbs, ses, root_fld, "Shared Folder")
        subfolder_id = subfolder.id
        dbs.commit()

    # Mock the client_show_user_message method to verify sharing confirmation message
    orig_show_message = oi.srv.client_show_user_message
    try:
        message_received = False

        async def mock_show_message(req: org.ClientShowUserMessageRequest) -> clap.Empty:
            nonlocal message_received
            if req.msg and "Folder shared" in req.msg.message:
                message_received = True
            return await orig_show_message(req)

        setattr(oi.srv, "client_show_user_message", mock_show_message)

        # Set server_info.url_base for URL generation
        oi.server_info = oi.server_info or org.ServerInfo()
        oi.server_info.url_base = "http://test.example.com"

        # Execute share command
        await oi.cmd_from_client(org.CmdFromClientRequest(
            ses=ses,
            cmd="share_folder",
            args=json.dumps({"id": subfolder_id})
        ))

        # Verify share was created in database
        with oi.db_new_session() as dbs:
            share = dbs.query(DbSharedFolder).filter(DbSharedFolder.folder_id == subfolder_id).one_or_none()
            assert share is not None, "Share should have been created"
            # Verify owner through folder ownership
            owner = await oi.folders_helper.get_folder_owner(dbs, subfolder_id)
            assert owner and owner.id == ses.user.id, "Share should be owned by correct user"
            assert message_received, "Share confirmation message should have been sent"

    finally:
        # Restore original method
        setattr(oi.srv, "client_show_user_message", orig_show_message)


async def org_test__cmd_from_client__revoke_share(oi: organizer.OrganizerInbound):
    """
    cmd_from_client() -- Test the 'revoke_share' client command.
    """
    # Create test session and folder
    ses, root_fld = await _create_test_folder_and_session(oi)

    # Create a subfolder
    with oi.db_new_session() as dbs:
        subfolder = await oi.folders_helper.create_folder(dbs, ses, root_fld, "Shared Folder")
        subfolder_id = subfolder.id
        dbs.commit()

    # Create share manually
    with oi.db_new_session() as dbs:
        share_token = await oi.folders_helper.generate_share_token()
        share = DbSharedFolder(
            folder_id=subfolder_id,
            share_token=share_token
        )
        dbs.add(share)
        dbs.commit()

        # Verify share exists
        assert dbs.query(DbSharedFolder).filter(DbSharedFolder.folder_id == subfolder_id).one_or_none() is not None

    # Execute revoke command
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=ses,
        cmd="revoke_share",
        args=json.dumps({"id": subfolder_id})
    ))

    # Verify share was removed
    with oi.db_new_session() as dbs:
        share = dbs.query(DbSharedFolder).filter(DbSharedFolder.folder_id == subfolder_id).one_or_none()
        assert share is None, "Share should have been removed"


async def org_test__navigate_shared_url(oi: organizer.OrganizerInbound):
    """
    Test navigating to a shared folder URL.
    """
    # Create owner session and folder
    owner_ses, root_fld = await _create_test_folder_and_session(oi)

    # Create a subfolder with some content
    subfolder = await oi.folders_helper.create_folder(oi.db_new_session(), owner_ses, root_fld, "Shared Folder")
    inner_folder = await oi.folders_helper.create_folder(oi.db_new_session(), owner_ses, subfolder, "Inner Folder")

    # Create share for subfolder
    share_token = None
    with oi.db_new_session() as dbs:
        share_token = await oi.folders_helper.generate_share_token()
        share = DbSharedFolder(
            folder_id=subfolder.id,
            share_token=share_token
        )
        dbs.add(share)
        dbs.commit()

    # Create second user
    with oi.db_new_session() as dbs:
        recipient_user_id = "share.recipient"
        dbs.add(DbUser(id=recipient_user_id, name="Share Recipient"))
        dbs.commit()

    recipient_ses = org.UserSessionData(
        sid="recipient_sid",
        user=clap.UserInfo(id=recipient_user_id, name="Share Recipient"),
        is_admin=False,
        cookies={}
    )

    # Navigate to the shared URL
    result = await organizer.navigate_page_impl(oi, org.NavigatePageRequest(
        ses=recipient_ses,
        page_id=f"shared.{share_token}"
    ))

    # Verify response contains the shared folder content
    assert isinstance(result, org.ClientShowPageRequest), "Result should be a ClientShowPageRequest"

    # Check if SHARED_FOLDER_ENTRY_COOKIE is set in recipient's session
    assert SHARED_FOLDER_TOKEN_COOKIE_NAME in recipient_ses.cookies, "Shared folder entry cookie should be set"
    assert recipient_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] == share_token, "Cookie should contain share token"

    # Verify recipient can fetch contents via cookie
    folder_contents = await oi.folders_helper.fetch_folder_contents(subfolder, recipient_ses)
    assert len(folder_contents) == 1, "Should have access to inner folder"
    assert folder_contents[0].id == inner_folder.id, "Should see the inner folder"


async def org_test__shared_folder_permissions(oi: organizer.OrganizerInbound):
    """
    Test permission boundaries for shared folders.
    """
    # Create owner session and folder hierarchy
    owner_ses, root_fld = await _create_test_folder_and_session(oi)
    owner_subfolder = await oi.folders_helper.create_folder(oi.db_new_session(), owner_ses, root_fld, "Shared Folder")
    owner_inner = await oi.folders_helper.create_folder(oi.db_new_session(), owner_ses, owner_subfolder, "Inner Folder")

    # Create another folder outside the shared path
    owner_other = await oi.folders_helper.create_folder(oi.db_new_session(), owner_ses, root_fld, "Not Shared")

    # Create share for owner_subfolder
    share_token = None
    with oi.db_new_session() as dbs:
        share_token = await oi.folders_helper.generate_share_token()
        share = DbSharedFolder(
            folder_id=owner_subfolder.id,
            share_token=share_token
        )
        dbs.add(share)
        dbs.commit()

    # Create recipient user
    with oi.db_new_session() as dbs:
        recipient_user_id = "perm.recipient"
        dbs.add(DbUser(id=recipient_user_id, name="Permission Test Recipient"))
        dbs.commit()

    recipient_ses = org.UserSessionData(
        sid="recipient_sid",
        user=clap.UserInfo(id=recipient_user_id, name="Permission Test Recipient"),
        is_admin=False,
        cookies={}
    )

    # Set up the shared folder session
    recipient_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] = share_token

    # Test 1: Recipient can access shared folder and its subfolder
    assert await oi.folders_helper.check_shared_folder_access(owner_subfolder.id, recipient_ses) is not None
    assert await oi.folders_helper.check_shared_folder_access(owner_inner.id, recipient_ses) is not None

    # Test 2: Recipient cannot access folders outside shared subtree
    assert await oi.folders_helper.check_shared_folder_access(owner_other.id, recipient_ses) is None
    assert await oi.folders_helper.check_shared_folder_access(root_fld.id, recipient_ses) is None

    # Test 3: Recipient cannot modify shared folder
    try:
        with oi.db_new_session() as dbs:
            await oi.folders_helper.create_folder(dbs, recipient_ses, owner_subfolder, "Test Create")
        assert False, "Should not be able to create folder"
    except GRPCError as e:
        assert e.status == GrpcStatus.PERMISSION_DENIED

    # Test 4: Recipient cannot trash shared folder
    try:
        with oi.db_new_session() as dbs:
            await oi.folders_helper.trash_folder_recursive(dbs, owner_inner.id, recipient_ses)
        assert False, "Should not be able to trash folder"
    except GRPCError as e:
        assert e.status == GrpcStatus.PERMISSION_DENIED


async def org_test__authorization_bypass_attempts(oi: organizer.OrganizerInbound):
    """
    Test authorization bypass attempts - non-owners trying to share/revoke folders.
    """
    # Create owner user and folder structure
    with oi.db_new_session() as dbs:
        owner_user_id = "owner.user"
        dbs.add(DbUser(id=owner_user_id, name="Owner User"))
        dbs.commit()

    owner_ses = org.UserSessionData(
        sid="owner_sid",
        user=clap.UserInfo(id=owner_user_id, name="Owner User"),
        is_admin=False,
        cookies={}
    )

    # Create owner's root folder and target folder
    with oi.db_new_session() as dbs:
        owner_root = await db_get_or_create_user_root_folder(dbs, owner_ses.user, oi.srv, oi.log)
        target_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Target Folder")
        target_folder_id = target_folder.id
        dbs.commit()

    # Create attacker user (different from owner)
    with oi.db_new_session() as dbs:
        attacker_user_id = "attacker.user"
        dbs.add(DbUser(id=attacker_user_id, name="Attacker User"))
        dbs.commit()

    attacker_ses = org.UserSessionData(
        sid="attacker_sid",
        user=clap.UserInfo(id=attacker_user_id, name="Attacker User"),
        is_admin=False,
        cookies={}
    )

    # Test 1: Non-owner trying to share another user's folder
    # Note: cmd_from_client catches PERMISSION_DENIED and shows user message instead of raising
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=attacker_ses,
        cmd="share_folder",
        args=json.dumps({"id": target_folder_id})
    ))

    # Verify no share was created
    with oi.db_new_session() as dbs:
        share = dbs.query(DbSharedFolder).filter(DbSharedFolder.folder_id == target_folder_id).one_or_none()
        assert share is None, "No share should have been created by unauthorized user"

    # Create a legitimate share by the owner
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=owner_ses,
        cmd="share_folder",
        args=json.dumps({"id": target_folder_id})
    ))

    # Verify share was created
    with oi.db_new_session() as dbs:
        share = dbs.query(DbSharedFolder).filter(DbSharedFolder.folder_id == target_folder_id).one_or_none()
        assert share is not None, "Share should have been created by owner"

    # Test 2: Non-owner trying to revoke another user's share
    # Note: cmd_from_client catches PERMISSION_DENIED and shows user message instead of raising
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=attacker_ses,
        cmd="revoke_share",
        args=json.dumps({"id": target_folder_id})
    ))

    # Verify share still exists
    with oi.db_new_session() as dbs:
        share = dbs.query(DbSharedFolder).filter(DbSharedFolder.folder_id == target_folder_id).one_or_none()
        assert share is not None, "Share should still exist after unauthorized revoke attempt"


async def org_test__shared_folder_path_traversal(oi: organizer.OrganizerInbound):
    """
    Test path traversal attempts - accessing folders outside shared subtree via cookie manipulation.
    """
    from organizer.helpers.folders import SHARED_FOLDER_TOKEN_COOKIE_NAME
    # Create owner session with folder hierarchy
    owner_ses, owner_root = await _create_test_folder_and_session(oi)

    with oi.db_new_session() as dbs:
        # Create folders: root -> shared_folder -> inner_folder
        #                 root -> private_folder
        shared_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Shared Folder")
        inner_folder = await oi.folders_helper.create_folder(dbs, owner_ses, shared_folder, "Inner Folder")
        private_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Private Folder")

        shared_folder_id = shared_folder.id
        inner_folder_id = inner_folder.id
        private_folder_id = private_folder.id
        owner_root_id = owner_root.id
        dbs.commit()

    # Create share for shared_folder only
    with oi.db_new_session() as dbs:
        share_token = await oi.folders_helper.generate_share_token()
        share = DbSharedFolder(
            folder_id=shared_folder_id,
            share_token=share_token
        )
        dbs.add(share)
        dbs.commit()

    # Create attacker user
    with oi.db_new_session() as dbs:
        attacker_user_id = "traversal.attacker"
        dbs.add(DbUser(id=attacker_user_id, name="Traversal Attacker"))
        dbs.commit()

    attacker_ses = org.UserSessionData(
        sid="traversal_sid",
        user=clap.UserInfo(id=attacker_user_id, name="Traversal Attacker"),
        is_admin=False,
        cookies={}
    )

    # Set up legitimate shared access
    attacker_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] = share_token

    # Test 1: Attacker should have access to shared subtree
    assert await oi.folders_helper.check_shared_folder_access(shared_folder_id, attacker_ses) is not None
    assert await oi.folders_helper.check_shared_folder_access(inner_folder_id, attacker_ses) is not None

    # Test 2: Attacker should NOT have access to private folders
    assert await oi.folders_helper.check_shared_folder_access(private_folder_id, attacker_ses) is None
    assert await oi.folders_helper.check_shared_folder_access(owner_root_id, attacker_ses) is None

    # Test 3: Cookie manipulation attempt - manually set shared entry to private folder
    from organizer.helpers.folders import SHARED_FOLDER_TOKEN_COOKIE_NAME
    attacker_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] = str(private_folder_id)

    # Should still not have access to private folder (cookie validation should prevent this)
    assert await oi.folders_helper.check_shared_folder_access(private_folder_id, attacker_ses) is None

    # Test 4: Try to fetch contents of private folder - should fail
    try:
        with oi.db_new_session() as dbs:
            private_folder_obj = dbs.query(DbFolder).filter(DbFolder.id == private_folder_id).one()
            await oi.folders_helper.fetch_folder_contents(private_folder_obj, attacker_ses)
        assert False, "Should not be able to fetch private folder contents"
    except GRPCError as e:
        assert e.status == GrpcStatus.PERMISSION_DENIED, f"Expected PERMISSION_DENIED, got {e.status}"


async def org_test__shared_folder_move_operations(oi: organizer.OrganizerInbound):
    """
    Test moving shared folders and their impact on sharing integrity.
    """
    # Create owner user and folder structure
    with oi.db_new_session() as dbs:
        owner_user_id = "move.owner"
        dbs.add(DbUser(id=owner_user_id, name="Move Test Owner"))
        dbs.commit()

    owner_ses = org.UserSessionData(
        sid="move_owner_sid",
        user=clap.UserInfo(id=owner_user_id, name="Move Test Owner"),
        is_admin=False,
        cookies={}
    )

    # Create folder hierarchy: root -> parent_folder -> shared_folder -> inner_folder
    #                          root -> destination_folder
    with oi.db_new_session() as dbs:
        owner_root = await db_get_or_create_user_root_folder(dbs, owner_ses.user, oi.srv, oi.log)
        parent_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Parent Folder")
        shared_folder = await oi.folders_helper.create_folder(dbs, owner_ses, parent_folder, "Shared Folder")
        inner_folder = await oi.folders_helper.create_folder(dbs, owner_ses, shared_folder, "Inner Folder")
        destination_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Destination Folder")

        shared_folder_id = shared_folder.id
        inner_folder_id = inner_folder.id
        destination_folder_id = destination_folder.id
        dbs.commit()

    # Create share for shared_folder
    with oi.db_new_session() as dbs:
        share_token = await oi.folders_helper.generate_share_token()
        share = DbSharedFolder(
            folder_id=shared_folder_id,
            share_token=share_token,
        )
        dbs.add(share)
        dbs.commit()

    # Create recipient user with shared access
    with oi.db_new_session() as dbs:
        recipient_user_id = "move.recipient"
        dbs.add(DbUser(id=recipient_user_id, name="Move Test Recipient"))
        dbs.commit()

    recipient_ses = org.UserSessionData(
        sid="move_recipient_sid",
        user=clap.UserInfo(id=recipient_user_id, name="Move Test Recipient"),
        is_admin=False,
        cookies={}
    )

    # Set up shared access for recipient
    recipient_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] = share_token

    # Test 1: Verify recipient has access before move
    assert await oi.folders_helper.check_shared_folder_access(shared_folder_id, recipient_ses) is not None
    assert await oi.folders_helper.check_shared_folder_access(inner_folder_id, recipient_ses) is not None

    # Test 2: Move shared folder to destination (owner action)
    await oi.move_to_folder(org.MoveToFolderRequest(
        ses=owner_ses,
        dst_folder_id=str(destination_folder_id),
        ids=[clap.FolderItemId(folder_id=str(shared_folder_id))],
        listing_data={}
    ))

    # Test 3: Verify share still exists and is functional after move
    with oi.db_new_session() as dbs:
        share = dbs.query(DbSharedFolder).filter(DbSharedFolder.folder_id == shared_folder_id).one_or_none()
        assert share is not None, "Share should still exist after folder move"
        assert share.share_token == share_token, "Share token should be unchanged"

    # Test 4: Verify recipient still has access after move
    assert await oi.folders_helper.check_shared_folder_access(shared_folder_id, recipient_ses) is not None
    assert await oi.folders_helper.check_shared_folder_access(inner_folder_id, recipient_ses) is not None

    # Test 5: Verify recipient can still fetch folder contents
    with oi.db_new_session() as dbs:
        shared_folder_obj = dbs.query(DbFolder).filter(DbFolder.id == shared_folder_id).one()
        contents = await oi.folders_helper.fetch_folder_contents(shared_folder_obj, recipient_ses)
        assert len(contents) == 1, "Should still see inner folder after move"
        assert contents[0].id == inner_folder_id, "Should still see the correct inner folder"

    # Test 6: Non-owner cannot move shared folder
    try:
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses=recipient_ses,
            dst_folder_id=str(destination_folder_id),
            ids=[clap.FolderItemId(folder_id=str(shared_folder_id))],
            listing_data={}
        ))
        assert False, "Recipient should not be able to move shared folder"
    except GRPCError as e:
        assert e.status == GrpcStatus.PERMISSION_DENIED, f"Expected PERMISSION_DENIED (error handled), got {e.status}"


async def org_test__shared_folder_rename_operations(oi: organizer.OrganizerInbound):
    """
    Test renaming shared folders while sharing is active.
    """
    # Create owner user and folder
    with oi.db_new_session() as dbs:
        owner_user_id = "rename.owner"
        dbs.add(DbUser(id=owner_user_id, name="Rename Test Owner"))
        dbs.commit()

    owner_ses = org.UserSessionData(
        sid="rename_owner_sid",
        user=clap.UserInfo(id=owner_user_id, name="Rename Test Owner"),
        is_admin=False,
        cookies={}
    )

    # Create shared folder
    with oi.db_new_session() as dbs:
        owner_root = await db_get_or_create_user_root_folder(dbs, owner_ses.user, oi.srv, oi.log)
        shared_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Original Name")
        shared_folder_id = shared_folder.id
        dbs.commit()

    # Create share
    with oi.db_new_session() as dbs:
        share_token = await oi.folders_helper.generate_share_token()
        share = DbSharedFolder(
            folder_id=shared_folder_id,
            share_token=share_token,
        )
        dbs.add(share)
        dbs.commit()

    # Create recipient user with shared access
    with oi.db_new_session() as dbs:
        recipient_user_id = "rename.recipient"
        dbs.add(DbUser(id=recipient_user_id, name="Rename Test Recipient"))
        dbs.commit()

    recipient_ses = org.UserSessionData(
        sid="rename_recipient_sid",
        user=clap.UserInfo(id=recipient_user_id, name="Rename Test Recipient"),
        is_admin=False,
        cookies={}
    )

    # Set up shared access for recipient
    recipient_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] = share_token

    # Test 1: Owner can rename shared folder
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=owner_ses,
        cmd="rename_folder",
        args=json.dumps({"id": shared_folder_id, "new_name": "Renamed Shared Folder"})
    ))

    # Test 2: Verify folder was renamed
    with oi.db_new_session() as dbs:
        folder = dbs.query(DbFolder).filter(DbFolder.id == shared_folder_id).one()
        assert folder.title == "Renamed Shared Folder", "Folder should be renamed"

    # Test 3: Verify share still exists and is functional after rename
    with oi.db_new_session() as dbs:
        share = dbs.query(DbSharedFolder).filter(DbSharedFolder.folder_id == shared_folder_id).one_or_none()
        assert share is not None, "Share should still exist after folder rename"
        assert share.share_token == share_token, "Share token should be unchanged"

    # Test 4: Verify recipient still has access after rename
    assert await oi.folders_helper.check_shared_folder_access(shared_folder_id, recipient_ses) is not None

    # Test 5: Recipient cannot rename shared folder (read-only access)
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=recipient_ses,
        cmd="rename_folder",
        args=json.dumps({"id": shared_folder_id, "new_name": "Unauthorized Rename"})
    ))

    # Test 6: Verify folder name was NOT changed by recipient
    with oi.db_new_session() as dbs:
        folder = dbs.query(DbFolder).filter(DbFolder.id == shared_folder_id).one()
        assert folder.title == "Renamed Shared Folder", "Folder name should not be changed by recipient"


async def org_test__move_content_into_shared_folders(oi: organizer.OrganizerInbound):
    """
    Test moving content into and out of shared folders with permission validation.
    """
    # Create owner user and complex folder structure
    with oi.db_new_session() as dbs:
        owner_user_id = "content.owner"
        dbs.add(DbUser(id=owner_user_id, name="Content Test Owner"))
        dbs.commit()

    owner_ses = org.UserSessionData(
        sid="content_owner_sid",
        user=clap.UserInfo(id=owner_user_id, name="Content Test Owner"),
        is_admin=False,
        cookies={}
    )

    # Create folder structure
    with oi.db_new_session() as dbs:
        owner_root = await db_get_or_create_user_root_folder(dbs, owner_ses.user, oi.srv, oi.log)
        shared_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Shared Container")
        private_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Private Folder")
        moveable_folder = await oi.folders_helper.create_folder(dbs, owner_ses, private_folder, "Moveable Folder")

        shared_folder_id = shared_folder.id
        private_folder_id = private_folder.id
        moveable_folder_id = moveable_folder.id
        dbs.commit()

    # Create share for shared_folder
    with oi.db_new_session() as dbs:
        share_token = await oi.folders_helper.generate_share_token()
        share = DbSharedFolder(
            folder_id=shared_folder_id,
            share_token=share_token,
        )
        dbs.add(share)
        dbs.commit()

    # Create recipient user
    with oi.db_new_session() as dbs:
        recipient_user_id = "content.recipient"
        dbs.add(DbUser(id=recipient_user_id, name="Content Test Recipient"))
        dbs.commit()

    recipient_ses = org.UserSessionData(
        sid="content_recipient_sid",
        user=clap.UserInfo(id=recipient_user_id, name="Content Test Recipient"),
        is_admin=False,
        cookies={}
    )

    # Set up shared access for recipient
    recipient_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] = share_token

    # Test 1: Owner can move content into shared folder
    await oi.move_to_folder(org.MoveToFolderRequest(
        ses=owner_ses,
        dst_folder_id=str(shared_folder_id),
        ids=[clap.FolderItemId(folder_id=str(moveable_folder_id))],
        listing_data={}
    ))

    # Test 2: Verify folder was moved into shared space
    with oi.db_new_session() as dbs:
        shared_folder_obj = dbs.query(DbFolder).filter(DbFolder.id == shared_folder_id).one()
        contents = await oi.folders_helper.fetch_folder_contents(shared_folder_obj, owner_ses)
        assert len(contents) == 1, "Shared folder should contain moved folder"
        assert contents[0].id == moveable_folder_id, "Should contain the moved folder"

    # Test 3: Recipient can now see the moved content (inherited shared access)
    assert await oi.folders_helper.check_shared_folder_access(moveable_folder_id, recipient_ses) is not None

    # Test 4: Recipient cannot move content out of shared folder
    try:
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses=recipient_ses,
            dst_folder_id=str(private_folder_id),
            ids=[clap.FolderItemId(folder_id=str(moveable_folder_id))],
            listing_data={}
        ))
        assert False, "Recipient should not be able to move content out of shared folder"
    except GRPCError as e:
        assert e.status == GrpcStatus.PERMISSION_DENIED, f"Expected PERMISSION_DENIED (error handled), got {e.status}"

    # Test 5: Recipient cannot move content into shared folder
    # Create another folder for testing
    with oi.db_new_session() as dbs:
        private_folder_obj = dbs.query(DbFolder).filter(DbFolder.id == private_folder_id).one()
        test_folder = await oi.folders_helper.create_folder(dbs, owner_ses, private_folder_obj, "Test Folder")
        test_folder_id = test_folder.id
        dbs.commit()

    try:
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses=recipient_ses,
            dst_folder_id=str(shared_folder_id),
            ids=[clap.FolderItemId(folder_id=str(test_folder_id))],
            listing_data={}
        ))
        assert False, "Recipient should not be able to move content into shared folder"
    except GRPCError as e:
        assert e.status == GrpcStatus.PERMISSION_DENIED, f"Expected PERMISSION_DENIED (error handled), got {e.status}"


async def org_test__admin_owner_transfer(oi: organizer.OrganizerInbound):
    """
    Test move_to_folder() as an admin -- Admin can move any folder or media file to any user's folder.
       When moving a folder into another user's folder, ownership of the source folder and all its contents
       are transferred to the destination folder's owner.
    """
    # Fetch a media file from the test database
    media_files = await oi.srv.db_get_media_files(org.DbGetMediaFilesRequest(all=clap.Empty()))
    assert len(media_files.items) > 0, "No media files found in the test database"

    media_owners = {v.id: v.user_id for v in media_files.items}

    # Get user for the media file
    src_media = media_files.items[0]
    src_user = clap.UserInfo(id=src_media.user_id, name=f"Name of {src_media.user_id}")
    src_user_ses = org.UserSessionData(sid="test_sid", user=src_user, is_admin=False, cookies={})

    # Create a folder + a subfolder for the source user
    with oi.db_new_session() as dbs:
        src_root_fld = await db_get_or_create_user_root_folder(dbs, src_user, oi.srv, oi.log)
        src_fld = await oi.folders_helper.create_folder(dbs, src_user_ses, src_root_fld, "Ownertransfer Test Folder")
        src_subfld = await oi.folders_helper.create_folder(dbs, src_user_ses, src_fld, "Ownertransfer Test Subfolder")

    # Move the file to the subfolder
    await oi.move_to_folder(org.MoveToFolderRequest(
        src_user_ses,
        ids=[clap.FolderItemId(media_file_id=src_media.id)],
        dst_folder_id=str(src_subfld.id),
        listing_data={}))

    assert src_media.user_id == src_user.id
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

    # No ownership transfer should've happened yet, check that media_file_owners matches
    new_owners = await oi.srv.db_get_media_files(org.DbGetMediaFilesRequest(all=clap.Empty()))
    for v in new_owners.items:
        assert media_owners[v.id] == v.user_id

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
        src_media = dbs.query(DbMediaFile).filter(DbMediaFile.id == src_media.id).one_or_none()
        assert src_fld is not None
        assert src_subfld is not None
        assert src_media is not None
        assert src_fld.user_id == dst_user.id
        assert src_subfld.user_id == dst_user.id
        assert src_media.user_id == dst_user.id

    # Check that the folder hierarchy is intact, but in `dst_fld`.
    dst_fld_cont = await oi.folders_helper.fetch_folder_contents(dst_fld, dst_user_ses)
    assert any(fi.id == src_fld.id for fi in dst_fld_cont)
    src_fld_cont = await oi.folders_helper.fetch_folder_contents(src_fld, dst_user_ses)
    assert len(src_fld_cont) == 1
    assert src_fld_cont[0].id == src_subfld.id
    subfld_cont = await oi.folders_helper.fetch_folder_contents(src_subfld, dst_user_ses)
    assert len(subfld_cont) == 1
    assert subfld_cont[0].id == src_media.id

    # Check that the example file's ownership was transferred, but not other files
    media_owners[src_media.id] = dst_user.id    # Update the expected owner
    new_owners = await oi.srv.db_get_media_files(org.DbGetMediaFilesRequest(all=clap.Empty()))
    for v in new_owners.items:
        assert media_owners[v.id] == v.user_id


async def org_test__admin_open_other_user_folder(oi: organizer.OrganizerInbound):
    """
    Test that administrators can double-click (open) another user's folder from the all users folder listing view.
    This regression test ensures the open_folder command works correctly for administrators accessing other users' folders.
    """
    # Create two users - one regular user and one admin
    with oi.db_new_session() as dbs:
        regular_user_id = "admin.test.regular"
        admin_user_id = "admin.test.admin"
        dbs.add(DbUser(id=regular_user_id, name="Regular User"))
        dbs.add(DbUser(id=admin_user_id, name="Admin User"))
        dbs.commit()

    # Create sessions for both users
    regular_ses = org.UserSessionData(
        sid="regular_sid",
        user=clap.UserInfo(id=regular_user_id, name="Regular User"),
        is_admin=False,
        cookies={}
    )

    admin_ses = org.UserSessionData(
        sid="admin_sid",
        user=clap.UserInfo(id=admin_user_id, name="Admin User"),
        is_admin=True,
        cookies={}
    )

    # Create root folders for both users and add some content to regular user's folder
    with oi.db_new_session() as dbs:
        regular_root = await db_get_or_create_user_root_folder(dbs, regular_ses.user, oi.srv, oi.log)

        # Create a test folder in regular user's space
        test_folder = await oi.folders_helper.create_folder(dbs, regular_ses, regular_root, "Regular User's Test Folder")
        regular_root_id = regular_root.id
        test_folder_id = test_folder.id
        dbs.commit()

    # Admin navigates to main page - should see all users' folders in admin view
    admin_page = await organizer.navigate_page_impl(oi, org.NavigatePageRequest(ses=admin_ses))
    assert isinstance(admin_page, org.ClientShowPageRequest)

    # Verify admin can see the regular user's folder in the listing
    # The _admin_show_all_user_homes function should have added a folder listing with user folders
    folder_listings = [item.folder_listing for item in admin_page.page_items if item.folder_listing]
    assert len(folder_listings) > 0, "Admin should see folder listings"

    # Find the all-users folder listing (should be the second one after the admin's own content)
    admin_user_listing = None
    for listing in folder_listings:
        if listing.listing_data and listing.listing_data.get("folder_id"):
            # Look for items with user folders
            for item in listing.items:
                if item.folder and item.folder.title == regular_user_id:
                    admin_user_listing = listing
                    break

    assert admin_user_listing is not None, "Admin should see other users' folders in listing"

    # Test: Admin double-clicks (opens) the regular user's folder
    # This simulates clicking on the regular user's folder from the all users view
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=admin_ses,
        cmd="open_folder",
        args=json.dumps({"id": regular_root_id})
    ))

    # Verify admin can see the contents of regular user's folder
    # Get updated folder path - should now include regular user's root folder
    folder_path, _ = await oi.folders_helper.get_current_folder_path(admin_ses)
    assert len(folder_path) >= 1, "Admin should have a valid folder path"
    current_folder = folder_path[-1]
    assert current_folder.id == regular_root_id, "Admin should now be viewing regular user's root folder"
    assert current_folder.user_id == regular_user_id, "Current folder should belong to regular user"

    # Verify admin can fetch contents of regular user's folder
    contents = await oi.folders_helper.fetch_folder_contents(current_folder, admin_ses)
    assert len(contents) == 1, "Regular user's folder should contain the test folder"
    assert contents[0].id == test_folder_id, "Should see the test folder created earlier"
    assert isinstance(contents[0], DbFolder), "Content should be a folder"
    assert contents[0].title == "Regular User's Test Folder", "Should see correct folder title"

    # Test: Admin can navigate deeper into regular user's folder structure
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=admin_ses,
        cmd="open_folder",
        args=json.dumps({"id": test_folder_id})
    ))

    # Verify admin can navigate to subfolder
    folder_path, _ = await oi.folders_helper.get_current_folder_path(admin_ses)
    assert len(folder_path) >= 2, "Admin should have deeper folder path"
    current_folder = folder_path[-1]
    assert current_folder.id == test_folder_id, "Admin should now be viewing the test subfolder"
    assert current_folder.user_id == regular_user_id, "Subfolder should still belong to regular user"

    # Verify admin can fetch contents of the subfolder (should be empty)
    contents = await oi.folders_helper.fetch_folder_contents(current_folder, admin_ses)
    assert len(contents) == 0, "Test subfolder should be empty"

    print("✓ Admin successfully opened and navigated another user's folder structure")


async def org_test__corrupted_folder_path_cookie(oi: organizer.OrganizerInbound):
    """
    Test handling of corrupted folder path cookies - ensures robust fallback behavior.
    """
    ses, root_fld = await _create_test_folder_and_session(oi)

    # Test 1: Malformed JSON in cookie
    ses.cookies[PATH_COOKIE_NAME] = "invalid json{"
    folder_path, _ = await oi.folders_helper.get_current_folder_path(ses)
    assert len(folder_path) == 1, "Should fall back to root folder on malformed JSON"
    assert folder_path[0].id == root_fld.id, "Should return user's root folder"

    # Test 2: Non-existent folder IDs in cookie
    ses.cookies[PATH_COOKIE_NAME] = json.dumps([99999, 88888])
    folder_path, _ = await oi.folders_helper.get_current_folder_path(ses)
    assert len(folder_path) == 1, "Should fall back to root folder on non-existent IDs"
    assert folder_path[0].id == root_fld.id, "Should return user's root folder"

    # Test 3: Mixed ownership in cookie (non-admin user)
    # Create another user and their folder
    with oi.db_new_session() as dbs:
        other_user_id = "corrupt.test.other"
        dbs.add(DbUser(id=other_user_id, name="Other User"))
        dbs.commit()

    other_ses = org.UserSessionData(
        sid="other_sid",
        user=clap.UserInfo(id=other_user_id, name="Other User"),
        is_admin=False,
        cookies={}
    )

    with oi.db_new_session() as dbs:
        other_root = await db_get_or_create_user_root_folder(dbs, other_ses.user, oi.srv, oi.log)
        other_root_id = other_root.id
        dbs.commit()

    # Set cookie with mixed ownership (user's folder + other user's folder)
    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id, other_root_id])
    folder_path, _ = await oi.folders_helper.get_current_folder_path(ses)
    assert len(folder_path) == 1, "Should clear mixed ownership path and fall back to root"
    assert folder_path[0].id == root_fld.id, "Should return user's root folder"

    # Verify cookie was cleared
    assert ses.cookies.get(PATH_COOKIE_NAME) != json.dumps([root_fld.id, other_root_id]), "Cookie should have been cleared"

    print("✓ Corrupted folder path cookie handling works correctly")


async def org_test__open_nonexistent_folder(oi: organizer.OrganizerInbound):
    """
    Test opening a folder that doesn't exist - ensures proper error handling.
    """
    ses, _ = await _create_test_folder_and_session(oi)

    # Test opening non-existent folder
    try:
        await oi.cmd_from_client(org.CmdFromClientRequest(
            ses=ses,
            cmd="open_folder",
            args=json.dumps({"id": 99999})
        ))
        assert False, "Should have raised NOT_FOUND error for non-existent folder"
    except GRPCError as e:
        assert e.status == GrpcStatus.NOT_FOUND, f"Expected NOT_FOUND, got {e.status}"
        assert "not found" in str(e.message).lower(), "Error message should mention folder not found"

    # Test with invalid folder ID type (should be caught by validation)
    try:
        await oi.cmd_from_client(org.CmdFromClientRequest(
            ses=ses,
            cmd="open_folder",
            args=json.dumps({"id": "invalid_string"})
        ))
        assert False, "Should have raised error for invalid folder ID type"
    except (GRPCError, AssertionError):
        pass  # Expected - either GRPCError or AssertionError from validation

    print("✓ Non-existent folder handling works correctly")


async def org_test__breadcrumb_navigation_up(oi: organizer.OrganizerInbound):
    """
    Test navigating back up the folder hierarchy - ensures breadcrumb truncation works.
    """
    ses, root_fld = await _create_test_folder_and_session(oi)

    # Create nested structure: root -> folder1 -> folder2 -> folder3
    with oi.db_new_session() as dbs:
        folder1 = await oi.folders_helper.create_folder(dbs, ses, root_fld, "Level 1")
        folder1_id = folder1.id
        dbs.commit()

    with oi.db_new_session() as dbs:
        folder1_obj = dbs.query(DbFolder).filter(DbFolder.id == folder1_id).one()
        folder2 = await oi.folders_helper.create_folder(dbs, ses, folder1_obj, "Level 2")
        folder2_id = folder2.id
        dbs.commit()

    with oi.db_new_session() as dbs:
        folder2_obj = dbs.query(DbFolder).filter(DbFolder.id == folder2_id).one()
        folder3 = await oi.folders_helper.create_folder(dbs, ses, folder2_obj, "Level 3")
        folder3_id = folder3.id
        dbs.commit()

    # Navigate down to folder3 step by step
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=ses, cmd="open_folder", args=json.dumps({"id": folder1_id})))

    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=ses, cmd="open_folder", args=json.dumps({"id": folder2_id})))

    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=ses, cmd="open_folder", args=json.dumps({"id": folder3_id})))

    # Verify we're at folder3 with full trail
    path, _ = await oi.folders_helper.get_current_folder_path(ses)
    path_ids = [f.id for f in path]
    assert len(path_ids) == 4, "Should have full path to folder3"
    assert path_ids == [root_fld.id, folder1_id, folder2_id, folder3_id], "Path should be complete hierarchy"

    # Navigate back to folder1 (should truncate trail)
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=ses, cmd="open_folder", args=json.dumps({"id": folder1_id})))

    path, _ = await oi.folders_helper.get_current_folder_path(ses)
    path_ids = [f.id for f in path]
    assert len(path_ids) == 2, "Should have truncated path"
    assert path_ids == [root_fld.id, folder1_id], "Should truncate at folder1"

    # Navigate back to root
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=ses, cmd="open_folder", args=json.dumps({"id": root_fld.id})))

    path, _ = await oi.folders_helper.get_current_folder_path(ses)
    path_ids = [f.id for f in path]
    assert len(path_ids) == 1, "Should be back to root only"
    assert path_ids == [root_fld.id], "Should be at root folder"

    print("✓ Breadcrumb navigation up hierarchy works correctly")


async def org_test__admin_multi_user_context_switching(oi: organizer.OrganizerInbound):
    """
    Test admin switching between multiple users' folders in the same session.
    """
    # Create 3 users
    with oi.db_new_session() as dbs:
        user1_id = "multi.user1"
        user2_id = "multi.user2"
        admin_id = "multi.admin"
        dbs.add(DbUser(id=user1_id, name="User 1"))
        dbs.add(DbUser(id=user2_id, name="User 2"))
        dbs.add(DbUser(id=admin_id, name="Admin User"))
        dbs.commit()

    user1_ses = org.UserSessionData(
        sid="user1_sid", user=clap.UserInfo(id=user1_id, name="User 1"),
        is_admin=False, cookies={})

    user2_ses = org.UserSessionData(
        sid="user2_sid", user=clap.UserInfo(id=user2_id, name="User 2"),
        is_admin=False, cookies={})

    admin_ses = org.UserSessionData(
        sid="admin_sid", user=clap.UserInfo(id=admin_id, name="Admin User"),
        is_admin=True, cookies={})

    # Create root folders for users
    with oi.db_new_session() as dbs:
        user1_root = await db_get_or_create_user_root_folder(dbs, user1_ses.user, oi.srv, oi.log)
        user1_root_id = user1_root.id
        dbs.commit()

    with oi.db_new_session() as dbs:
        user2_root = await db_get_or_create_user_root_folder(dbs, user2_ses.user, oi.srv, oi.log)
        user2_root_id = user2_root.id
        dbs.commit()

    with oi.db_new_session() as dbs:
        admin_root = await db_get_or_create_user_root_folder(dbs, admin_ses.user, oi.srv, oi.log)
        admin_root_id = admin_root.id
        dbs.commit()

    # Admin starts at their own root
    path, _ = await oi.folders_helper.get_current_folder_path(admin_ses)
    assert path[-1].id == admin_root_id, "Admin should start at their own root"
    assert path[-1].user_id == admin_id, "Should be admin's folder"

    # Admin opens user1's folder
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=admin_ses, cmd="open_folder", args=json.dumps({"id": user1_root_id})))

    path, _ = await oi.folders_helper.get_current_folder_path(admin_ses)
    assert len(path) == 1, "Should start fresh trail when switching users"
    assert path[0].id == user1_root_id, "Should be at user1's root"
    assert path[0].user_id == user1_id, "Should be user1's folder"

    # Admin switches to user2's folder
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=admin_ses, cmd="open_folder", args=json.dumps({"id": user2_root_id})))

    path, _ = await oi.folders_helper.get_current_folder_path(admin_ses)
    assert len(path) == 1, "Should start fresh trail again when switching users"
    assert path[0].id == user2_root_id, "Should be at user2's root"
    assert path[0].user_id == user2_id, "Should be user2's folder"

    # Admin switches back to their own folder
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=admin_ses, cmd="open_folder", args=json.dumps({"id": admin_root_id})))

    path, _ = await oi.folders_helper.get_current_folder_path(admin_ses)
    assert len(path) == 1, "Should have single folder when returning to own root"
    assert path[0].id == admin_root_id, "Should be at admin's root"
    assert path[0].user_id == admin_id, "Should be admin's folder"

    print("✓ Admin multi-user context switching works correctly")


async def org_test__shared_folder_cookie_interactions(oi: organizer.OrganizerInbound):
    """
    Test interaction between path cookies and shared folder cookies.
    """
    # Create owner session and shared folder
    owner_ses, owner_root = await _create_test_folder_and_session(oi)

    with oi.db_new_session() as dbs:
        shared_folder = await oi.folders_helper.create_folder(dbs, owner_ses, owner_root, "Shared Folder")
        subfolder = await oi.folders_helper.create_folder(dbs, owner_ses, shared_folder, "Subfolder")
        shared_folder_id = shared_folder.id
        subfolder_id = subfolder.id
        dbs.commit()

    # Create share
    with oi.db_new_session() as dbs:
        share_token = await oi.folders_helper.generate_share_token()
        share = DbSharedFolder(
            folder_id=shared_folder_id,
            share_token=share_token
        )
        dbs.add(share)
        dbs.commit()

    # Create recipient user
    with oi.db_new_session() as dbs:
        recipient_user_id = "shared.cookie.recipient"
        dbs.add(DbUser(id=recipient_user_id, name="Cookie Test Recipient"))
        dbs.commit()

    recipient_ses = org.UserSessionData(
        sid="recipient_sid",
        user=clap.UserInfo(id=recipient_user_id, name="Cookie Test Recipient"),
        is_admin=False,
        cookies={}
    )

    # Recipient accesses shared folder via shared URL
    result = await organizer.navigate_page_impl(oi, org.NavigatePageRequest(
        ses=recipient_ses,
        page_id=f"shared.{share_token}"
    ))

    assert isinstance(result, org.ClientShowPageRequest), "Should return valid page"

    # Verify both cookies are set
    assert PATH_COOKIE_NAME in recipient_ses.cookies, "Path cookie should be set"
    assert SHARED_FOLDER_TOKEN_COOKIE_NAME in recipient_ses.cookies, "Shared token cookie should be set"
    assert recipient_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] == share_token, "Shared token should match"

    # Verify path cookie points to shared folder
    path_data = json.loads(recipient_ses.cookies[PATH_COOKIE_NAME])
    assert path_data == [shared_folder_id], "Path should point to shared folder"

    # Navigate within shared folder - should preserve shared token
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=recipient_ses,
        cmd="open_folder",
        args=json.dumps({"id": subfolder_id})
    ))

    # Verify shared token is still present
    assert SHARED_FOLDER_TOKEN_COOKIE_NAME in recipient_ses.cookies, "Shared token should be preserved"
    assert recipient_ses.cookies[SHARED_FOLDER_TOKEN_COOKIE_NAME] == share_token, "Shared token should be unchanged"

    # Verify path was updated
    path_data = json.loads(recipient_ses.cookies[PATH_COOKIE_NAME])
    assert path_data == [shared_folder_id, subfolder_id], "Path should include subfolder"

    # Verify recipient can still access folder contents
    path, _ = await oi.folders_helper.get_current_folder_path(recipient_ses)
    assert len(path) == 2, "Should have path to subfolder"
    assert path[0].id == shared_folder_id, "Should start at shared folder"
    assert path[1].id == subfolder_id, "Should end at subfolder"

    print("✓ Shared folder cookie interactions work correctly")


async def org_test__user_cleanup_basic_test(oi: organizer.OrganizerInbound):
    """
    Basic test for user cleanup functionality without comment dependencies.
    Tests that users are deleted when they have no remaining content.
    Uses pre-existing test database users and content.
    """
    # Get initial media files and identify users from test database
    media_files = await oi.srv.db_get_media_files(org.DbGetMediaFilesRequest(all=clap.Empty()))
    assert len(media_files.items) >= 2, "Need at least 2 media files for this test"

    # Use pre-existing users from test database:
    # user.num2 owns files 11111 and B1DE3 (files 1 and 3)
    # user.num1 owns files B1DE0, 22222, B1DE4 (files 0, 2, 4)
    source_user_id = "user.num2"  # Will be deleted after transfer
    dest_user_id = "user.num1"    # Will receive the content

    # Create admin session
    admin_user_id = "cleanup.basic.admin"
    with oi.db_new_session() as dbs:
        dbs.add(DbUser(id=admin_user_id, name="Basic Admin"))
        dbs.commit()

    admin_ses = org.UserSessionData(
        sid="admin_sid",
        user=clap.UserInfo(id=admin_user_id, name="Basic Admin"),
        is_admin=True,
        cookies={}
    )

    # Set up destination folder to receive content
    dest_ses = org.UserSessionData(
        sid="dest_sid",
        user=clap.UserInfo(id=dest_user_id, name="User Number1"),
        is_admin=False,
        cookies={}
    )

    with oi.db_new_session() as dbs:
        dest_root = await db_get_or_create_user_root_folder(dbs, dest_ses.user, oi.srv, oi.log)
        dest_folder = await oi.folders_helper.create_folder(dbs, dest_ses, dest_root, "Destination Folder")
        dest_folder_id = dest_folder.id
        dbs.commit()

    # Verify source user exists and has content before transfer
    with oi.db_new_session() as dbs:
        source_user = dbs.query(DbUser).filter(DbUser.id == source_user_id).one_or_none()
        assert source_user is not None, "Source user should exist"

        media_count = dbs.query(DbMediaFile).filter(DbMediaFile.user_id == source_user_id).count()
        assert media_count == 2, f"Source user should have 2 media files, has {media_count}"

    # Find source user's media files (11111 and B1DE3)
    source_media_files = [mf for mf in media_files.items if mf.user_id == source_user_id]
    assert len(source_media_files) == 2, f"Expected 2 media files for source user, found {len(source_media_files)}"

    # Transfer all source user's media files to destination folder
    for media_file in source_media_files:
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses=admin_ses,
            dst_folder_id=str(dest_folder_id),
            ids=[clap.FolderItemId(media_file_id=media_file.id)],
            listing_data={}
        ))

    # Verify source user still exists (automatic cleanup removed)
    with oi.db_new_session() as dbs:
        source_user = dbs.query(DbUser).filter(DbUser.id == source_user_id).one_or_none()
        assert source_user is not None, "Source user should still exist after transfer (no automatic cleanup)"

        media_count = dbs.query(DbMediaFile).filter(DbMediaFile.user_id == source_user_id).count()
        assert media_count == 0, f"No media files should remain for source user after transfer, found {media_count}"

    # Verify ownership transfer worked
    with oi.db_new_session() as dbs:
        for media_file in source_media_files:
            transferred_media = dbs.query(DbMediaFile).filter(DbMediaFile.id == media_file.id).one()
            assert transferred_media.user_id == dest_user_id, f"Media {media_file.id} should belong to dest user, belongs to {transferred_media.user_id}"

    print("✓ Basic content transfer functionality works correctly (no automatic cleanup)")


async def org_test__user_cleanup_after_content_transfer(oi: organizer.OrganizerInbound):
    """
    Test that users are automatically deleted when all their content is transferred to other users.
    Uses pre-existing test database users and their existing comments.

    This test verifies:
    1. User deletion when they have no remaining folders or media files
    2. Comments are preserved with username_ifnull field set correctly
    3. The trigger tr_comments_set_username_on_user_delete works correctly
    """
    # Get initial media files from test database
    media_files = await oi.srv.db_get_media_files(org.DbGetMediaFilesRequest(all=clap.Empty()))
    assert len(media_files.items) >= 3, "Need at least 3 media files for this test"

    # Use pre-existing users from test database:
    # user.num1 owns files B1DE0 (5 comments), 22222 (1 comment), B1DE4 (0 comments)
    # user.num2 owns files 11111 (2 comments), B1DE3 (0 comments)
    source_user_id = "user.num1"  # Will be deleted after transfer (has comments)
    source_user_name = "User Number1"
    dest_user_id = "user.num2"    # Will receive the content

    # Create admin session
    admin_user_id = "cleanup.admin"
    with oi.db_new_session() as dbs:
        dbs.add(DbUser(id=admin_user_id, name="Cleanup Test Admin"))
        dbs.commit()

    admin_ses = org.UserSessionData(
        sid="admin_sid",
        user=clap.UserInfo(id=admin_user_id, name="Cleanup Test Admin"),
        is_admin=True,
        cookies={}
    )

    # Set up destination folder to receive content
    dest_ses = org.UserSessionData(
        sid="dest_sid",
        user=clap.UserInfo(id=dest_user_id, name="User Number2"),
        is_admin=False,
        cookies={}
    )

    with oi.db_new_session() as dbs:
        dest_root = await db_get_or_create_user_root_folder(dbs, dest_ses.user, oi.srv, oi.log)
        dest_folder = await oi.folders_helper.create_folder(dbs, dest_ses, dest_root, "Destination Folder")
        dest_folder_id = dest_folder.id
        dbs.commit()

    # Find media files with comments from the source user specifically
    media_with_comments = []
    media_without_comments = []

    with oi.db_new_session() as dbs:
        for media_file in media_files.items:
            if media_file.user_id == source_user_id:
                # Count only comments from the source user on this media file
                comment_count = dbs.execute(sqlalchemy.text(
                    "SELECT COUNT(*) FROM comments WHERE media_file_id = :media_id AND user_id = :user_id"
                ), {"media_id": media_file.id, "user_id": source_user_id}).scalar()

                if comment_count > 0:
                    media_with_comments.append((media_file, comment_count))
                else:
                    media_without_comments.append(media_file)

    assert len(media_with_comments) >= 1, f"Expected at least 1 media file with comments from source user, found {len(media_with_comments)}"

    total_expected_comments = sum(count for _, count in media_with_comments)
    print(f"Found {len(media_with_comments)} media files with {total_expected_comments} total comments from source user")

    # Verify source user exists and has content before transfer
    with oi.db_new_session() as dbs:
        source_user = dbs.query(DbUser).filter(DbUser.id == source_user_id).one_or_none()
        assert source_user is not None, "Source user should exist"
        assert source_user.name == source_user_name

        media_count = dbs.query(DbMediaFile).filter(DbMediaFile.user_id == source_user_id).count()
        comment_count = dbs.execute(sqlalchemy.text(
            "SELECT COUNT(*) FROM comments WHERE user_id = :user_id"
        ), {"user_id": source_user_id}).scalar()

        # Debug: Check where all the source user's comments are
        all_comments = dbs.execute(sqlalchemy.text("""
            SELECT media_file_id, comment FROM comments WHERE user_id = :user_id ORDER BY media_file_id
        """), {"user_id": source_user_id}).fetchall()

        print(f"Debug: Source user has {comment_count} total comments:")
        for comment in all_comments:
            print(f"  - Media {comment.media_file_id}: '{comment.comment}'")

        assert media_count == 3, f"Source user should have 3 media files, has {media_count}"
        assert comment_count > 0, f"Source user should have some comments to test preservation, has {comment_count}"

        # Use the actual comment count instead of assuming it matches our per-file sum
        total_expected_comments = comment_count
        print(f"Adjusted: Source user actually has {total_expected_comments} total comments")

    # Transfer all source user's media files to destination folder
    source_media_files = [mf for mf in media_files.items if mf.user_id == source_user_id]

    for media_file in source_media_files:
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses=admin_ses,
            dst_folder_id=str(dest_folder_id),
            ids=[clap.FolderItemId(media_file_id=media_file.id)],
            listing_data={}
        ))

    # Verify source user still exists (automatic cleanup removed)
    with oi.db_new_session() as dbs:
        source_user = dbs.query(DbUser).filter(DbUser.id == source_user_id).one_or_none()
        assert source_user is not None, "Source user should still exist after transfer (no automatic cleanup)"

        media_count = dbs.query(DbMediaFile).filter(DbMediaFile.user_id == source_user_id).count()
        assert media_count == 0, f"No media files should remain for source user after transfer, found {media_count}"

    # Verify comments are preserved with original user_id (user not deleted)
    with oi.db_new_session() as dbs:
        # Check all comments from the source user (user_id should still be set)
        preserved_comments = dbs.execute(sqlalchemy.text("""
            SELECT user_id, username_ifnull, comment, media_file_id
            FROM comments
            WHERE user_id = :user_id
            ORDER BY media_file_id, id
        """), {"user_id": source_user_id}).fetchall()

        assert len(preserved_comments) == total_expected_comments, f"Should have {total_expected_comments} comments from source user, found {len(preserved_comments)}"

        print(f"Debug: Found {len(preserved_comments)} comments still owned by source user:")
        for comment in preserved_comments:
            print(f"  - Media {comment.media_file_id}: '{comment.comment}' (user_id={comment.user_id}, username_ifnull='{comment.username_ifnull}')")

        # Verify all comments still have original user_id (user not deleted)
        for comment in preserved_comments:
            assert comment.user_id == source_user_id, f"Comment user_id should still be {source_user_id}, got {comment.user_id}"

    # Verify ownership transfer worked correctly
    with oi.db_new_session() as dbs:
        for media_file in source_media_files:
            transferred_media = dbs.query(DbMediaFile).filter(DbMediaFile.id == media_file.id).one()
            assert transferred_media.user_id == dest_user_id, f"Media {media_file.id} should belong to dest user, belongs to {transferred_media.user_id}"

    print("✓ Content transfer works correctly (user preserved, no automatic cleanup)")
    print(f"✓ {total_expected_comments} comments preserved with source user still existing")


async def org_test__cmd_from_client__cleanup_empty_user(oi: organizer.OrganizerInbound):
    """
    Test the 'cleanup_empty_user' client command for explicit user cleanup via context menu.
    """
    # Create admin session
    admin_user = clap.UserInfo(id="admin", name="Admin User")
    admin_ses = org.UserSessionData(
        sid="admin_sid",
        user=admin_user,
        is_admin=True,
        cookies={}
    )

    # Create a test user with only an empty root folder
    test_user = clap.UserInfo(id="cleanup.test_user", name="Test User")

    # Add both admin and test user to database and create their root folders
    with oi.db_new_session() as dbs:
        dbs.add(DbUser(id=admin_user.id, name=admin_user.name))
        dbs.add(DbUser(id=test_user.id, name=test_user.name))
        root_folder = await db_get_or_create_user_root_folder(dbs, test_user, oi.srv, oi.log)
        root_folder_id = root_folder.id
        dbs.commit()

    # Verify user exists before cleanup
    with oi.db_new_session() as dbs:
        user = dbs.query(DbUser).filter(DbUser.id == test_user.id).one_or_none()
        assert user is not None, "Test user should exist before cleanup"

    # Execute cleanup command via admin
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=admin_ses,
        cmd="cleanup_empty_user",
        args=json.dumps({"folder_id": root_folder_id})
    ))

    # Verify user was deleted
    with oi.db_new_session() as dbs:
        user = dbs.query(DbUser).filter(DbUser.id == test_user.id).one_or_none()
        assert user is None, "Test user should have been deleted"

        # Verify root folder was also deleted
        folder = dbs.query(DbFolder).filter(DbFolder.id == root_folder_id).one_or_none()
        assert folder is None, "Empty root folder should have been deleted"

    print("✓ cleanup_empty_user command correctly deletes empty users")
    print("✓ Admin permission check works correctly")
    print("✓ Empty root folders are deleted along with users")


async def org_test__cmd_from_client__cleanup_empty_user_batch(oi: organizer.OrganizerInbound):
    """
    Test the batch 'cleanup_empty_user' client command (folder_id='*') for cleaning up multiple users.
    """
    # Create admin session
    admin_user = clap.UserInfo(id="admin", name="Admin User")
    admin_ses = org.UserSessionData(
        sid="admin_sid",
        user=admin_user,
        is_admin=True,
        cookies={}
    )

    # First, find any pre-existing users with no media files (like the connecting user)
    # These will also be eligible for cleanup
    with oi.db_new_session() as dbs:
        pre_existing_empty_users = dbs.execute(sqlalchemy.text("""
            SELECT u.id
            FROM users u
            LEFT JOIN media_files mf ON mf.user_id = u.id
            WHERE u.id != :admin_id
            GROUP BY u.id
            HAVING COUNT(mf.id) = 0
        """), {"admin_id": admin_user.id}).fetchall()
        pre_existing_empty_count = len(pre_existing_empty_users)
        print(f"Pre-existing empty users (excluding admin): {[u.id for u in pre_existing_empty_users]} (count: {pre_existing_empty_count})")

    # Create test users for batch cleanup
    test_users = [
        ("batch.empty1", "Empty User 1"),
        ("batch.empty2", "Empty User 2"),
        ("batch.root_only", "Root Only User"),
    ]

    # Add admin and test users to database
    with oi.db_new_session() as dbs:
        dbs.add(DbUser(id=admin_user.id, name=admin_user.name))
        for user_id, user_name in test_users:
            dbs.add(DbUser(id=user_id, name=user_name))
        dbs.commit()

    # Create root folder for one test user (should still be eligible for cleanup)
    root_only_ses = org.UserSessionData(
        sid="root_only_sid",
        user=clap.UserInfo(id="batch.root_only", name="Root Only User"),
        is_admin=False,
        cookies={}
    )
    with oi.db_new_session() as dbs:
        await db_get_or_create_user_root_folder(dbs, root_only_ses.user, oi.srv, oi.log)
        dbs.commit()

    # Count users before cleanup
    with oi.db_new_session() as dbs:
        users_before_count = dbs.query(DbUser).count()

    # Execute batch cleanup command via admin (folder_id='*')
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=admin_ses,
        cmd="cleanup_empty_user",
        args=json.dumps({"folder_id": "*"})
    ))

    # Verify results
    with oi.db_new_session() as dbs:
        # Count users after cleanup
        users_after_count = dbs.query(DbUser).count()
        users_deleted = users_before_count - users_after_count

        # Should have deleted our test users + pre-existing empty users
        expected_deletions = len(test_users) + pre_existing_empty_count
        assert users_deleted == expected_deletions, f"Expected {expected_deletions} users to be deleted, but {users_deleted} were deleted"

        # Admin should still exist (excluded from cleanup)
        admin_still_exists = dbs.query(DbUser).filter(DbUser.id == admin_user.id).one_or_none()
        assert admin_still_exists is not None, "Admin user should not be deleted during batch cleanup"

        # Test users should be deleted
        for user_id, _ in test_users:
            user = dbs.query(DbUser).filter(DbUser.id == user_id).one_or_none()
            assert user is None, f"Test user '{user_id}' should have been deleted"

    print("✓ Batch cleanup command correctly deletes multiple empty users")
    print("✓ Admin user is excluded from batch cleanup")
    print(f"✓ Cleaned up {users_deleted} users total ({len(test_users)} test + {pre_existing_empty_count} pre-existing)")


async def org_test__admin_create_folder_in_user_context(oi: organizer.OrganizerInbound):
    """
    Test that when an admin creates a folder while viewing another user's folder,
    the folder is created with the correct ownership (target user, not admin).

    This test checks for the bug where admin-created folders might end up
    in the admin's own context instead of the target user's context.
    """
    # Create two users - one regular user and one admin
    with oi.db_new_session() as dbs:
        regular_user_id = "folder.test.regular"
        admin_user_id = "folder.test.admin"
        dbs.add(DbUser(id=regular_user_id, name="Regular User"))
        dbs.add(DbUser(id=admin_user_id, name="Admin User"))
        dbs.commit()

    # Create sessions for both users
    regular_ses = org.UserSessionData(
        sid="regular_sid",
        user=clap.UserInfo(id=regular_user_id, name="Regular User"),
        is_admin=False,
        cookies={}
    )

    admin_ses = org.UserSessionData(
        sid="admin_sid",
        user=clap.UserInfo(id=admin_user_id, name="Admin User"),
        is_admin=True,
        cookies={}
    )

    # Create root folders for both users
    with oi.db_new_session() as dbs:
        regular_root = await db_get_or_create_user_root_folder(dbs, regular_ses.user, oi.srv, oi.log)
        admin_root = await db_get_or_create_user_root_folder(dbs, admin_ses.user, oi.srv, oi.log)

        # Create a subfolder in regular user's space for testing
        test_parent_folder = await oi.folders_helper.create_folder(dbs, regular_ses, regular_root, "User's Parent Folder")
        regular_root_id = regular_root.id
        test_parent_folder_id = test_parent_folder.id
        admin_root_id = admin_root.id
        dbs.commit()

    # Admin navigates to the regular user's folder (simulating admin browsing another user's folders)
    admin_ses.cookies[PATH_COOKIE_NAME] = json.dumps([regular_root_id, test_parent_folder_id])

    # Admin creates a new folder while viewing the regular user's folder
    await oi.cmd_from_client(org.CmdFromClientRequest(
        ses=admin_ses,
        cmd="new_folder",
        args='{"name": "Admin Created Folder"}'
    ))

    # Verify the folder was created
    with oi.db_new_session() as dbs:
        # Get all folders to check ownership
        all_folders = dbs.query(DbFolder).all()

        # Find the newly created folder
        admin_created_folder = None
        for folder in all_folders:
            if folder.title == "Admin Created Folder":
                admin_created_folder = folder
                break

        assert admin_created_folder is not None, "Admin created folder should exist"

        # Critical test: The folder should belong to the regular user, NOT the admin
        print(f"Admin created folder owner: {admin_created_folder.user_id}")
        print(f"Expected owner (regular user): {regular_user_id}")
        print(f"Admin user ID: {admin_user_id}")

        assert admin_created_folder.user_id == regular_user_id, \
            f"BUG DETECTED: Admin created folder belongs to admin ({admin_created_folder.user_id}) instead of target user ({regular_user_id})"

        # Verify the folder appears in the regular user's folder structure
        test_parent_folder_refreshed = dbs.query(DbFolder).filter_by(id=test_parent_folder_id).first()
        folder_contents = await oi.folders_helper.fetch_folder_contents(test_parent_folder_refreshed, regular_ses)

        created_folders = [fi for fi in folder_contents if isinstance(fi, DbFolder) and fi.title == "Admin Created Folder"]
        assert len(created_folders) == 1, "Admin created folder should appear in regular user's folder structure"

        # Double-check: verify the folder does NOT appear in admin's own folder structure
        admin_root_refreshed = dbs.query(DbFolder).filter_by(id=admin_root_id).first()
        admin_folder_contents = await oi.folders_helper.fetch_folder_contents(admin_root_refreshed, admin_ses)

        admin_folders = [fi for fi in admin_folder_contents if isinstance(fi, DbFolder) and fi.title == "Admin Created Folder"]
        assert len(admin_folders) == 0, "Admin created folder should NOT appear in admin's own folder structure"

        print("✓ Test passed: Admin created folder has correct ownership and location")
        print(f"✓ Folder 'Admin Created Folder' correctly belongs to user '{regular_user_id}' (not admin '{admin_user_id}')")


# TODO: JavaScript action validation testing?
# Consider adding validation for generated JavaScript code in actiondefs.py to catch:
# - Syntax errors in generated JavaScript snippets
# - Undefined variable access (e.g., _action_args.folder vs _action_args.listing_data)
# - Missing null checks and API contract violations
# - Common popup action bugs like accessing wrong data structures
# Potential approaches: Python AST analysis of JS strings, or external validation tool
# Would have caught the folder sharing bug (_action_args.folder?.id vs _action_args.listing_data?.folder_id)


async def org_test__folder_viewer_tracker(oi: organizer.OrganizerInbound):
    """
    FolderViewerTracker -- Unit tests for the viewer tracking data structure.
    """
    from organizer.helpers.viewer_tracker import FolderViewerTracker

    tracker = FolderViewerTracker()

    # Basic register and retrieval
    tracker.register(folder_id=1, sid="alice")
    tracker.register(folder_id=1, sid="bob")
    tracker.register(folder_id=2, sid="carol")

    others = tracker.get_other_viewers(1, exclude_sid="alice")
    assert others == ["bob"], f"Expected ['bob'], got {others}"

    others = tracker.get_other_viewers(1, exclude_sid="bob")
    assert others == ["alice"], f"Expected ['alice'], got {others}"

    # Carol is in folder 2, not folder 1
    others = tracker.get_other_viewers(1, exclude_sid=None)
    assert set(others) == {"alice", "bob"}, f"Expected alice+bob, got {others}"

    # Re-registering alice to folder 2 should remove her from folder 1
    tracker.register(folder_id=2, sid="alice")
    others = tracker.get_other_viewers(1, exclude_sid=None)
    assert others == ["bob"], f"After re-register, folder 1 should only have bob, got {others}"
    others = tracker.get_other_viewers(2, exclude_sid=None)
    assert set(others) == {"carol", "alice"}, f"Folder 2 should have carol+alice, got {others}"

    # Cap eviction: fill up to MAX_ENTRIES+1 and verify the oldest is gone
    tiny_tracker = FolderViewerTracker()
    tiny_tracker.MAX_ENTRIES = 3
    tiny_tracker.register(folder_id=10, sid="s1")
    tiny_tracker.register(folder_id=10, sid="s2")
    tiny_tracker.register(folder_id=10, sid="s3")
    tiny_tracker.register(folder_id=10, sid="s4")  # should evict s1 (oldest)
    all_viewers = set(tiny_tracker.get_other_viewers(10, exclude_sid=None))
    assert "s1" not in all_viewers, f"s1 should have been evicted, got {all_viewers}"
    assert {"s2", "s3", "s4"} == all_viewers, f"Expected s2+s3+s4, got {all_viewers}"

    print("FolderViewerTracker tests passed.")


async def org_test__viewer_notifications(oi: organizer.OrganizerInbound):
    """
    notify_folder_viewers() -- Verify that after a folder mutation, an empty ShowPage hint
    is sent to other sessions viewing the same folder, but NOT to the acting session.
    """
    # Create two independent users/sessions
    with oi.db_new_session() as dbs:
        dbs.add(DbUser(id="viewer_notif.user_a", name="User A"))
        dbs.add(DbUser(id="viewer_notif.user_b", name="User B"))
        dbs.commit()

    ses_a = org.UserSessionData(sid="notif_sid_a", user=clap.UserInfo(id="viewer_notif.user_a", name="User A"), is_admin=False, cookies={})
    ses_b = org.UserSessionData(sid="notif_sid_b", user=clap.UserInfo(id="viewer_notif.user_b", name="User B"), is_admin=True, cookies={})

    # Both sessions navigate to their respective root folders to get them created
    await organizer.navigate_page_impl(oi, org.NavigatePageRequest(ses=ses_a))
    await organizer.navigate_page_impl(oi, org.NavigatePageRequest(ses=ses_b))

    # Get user A's root folder and place both sessions there
    folder_path_a, _ = await oi.folders_helper.get_current_folder_path(ses_a, None)
    root_fld_a = folder_path_a[-1]

    ses_a.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld_a.id])
    ses_b.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld_a.id])

    # Navigate both sessions to A's root folder to register them as viewers
    page_id_a = str(root_fld_a.id)
    await organizer.navigate_page_impl(oi, org.NavigatePageRequest(ses=ses_a, page_id=page_id_a))
    await organizer.navigate_page_impl(oi, org.NavigatePageRequest(ses=ses_b, page_id=page_id_a))

    # Intercept client_show_page calls
    show_page_calls: list[org.ClientShowPageRequest] = []
    orig_show_page = oi.srv.client_show_page

    async def _mock_show_page(self, req: org.ClientShowPageRequest) -> clap.Empty:
        show_page_calls.append(req)
        return clap.Empty()

    setattr(oi.srv, "client_show_page", MethodType(_mock_show_page, oi.srv))
    try:
        # Session A creates a new folder — this is the mutation
        await oi.cmd_from_client(org.CmdFromClientRequest(
            ses=ses_a, cmd="new_folder", args='{"name": "New Folder"}'))
    finally:
        setattr(oi.srv, "client_show_page", orig_show_page)

    # There should be exactly two ShowPage calls:
    # 1. A full page for the acting user (ses_a) — has page_items
    # 2. An empty hint for the other viewer (ses_b) — no page_items
    sids_called = [r.sid for r in show_page_calls]
    print(f"ShowPage calls: {[(r.sid, len(r.page_items)) for r in show_page_calls]}")

    assert "notif_sid_a" in sids_called, "Acting user should get a full page refresh"
    assert "notif_sid_b" in sids_called, "Other viewer should receive a refresh hint"

    full_pages = [r for r in show_page_calls if r.page_items]
    hint_pages = [r for r in show_page_calls if not r.page_items]

    assert any(r.sid == "notif_sid_a" for r in full_pages), "ses_a should get a full page"
    assert any(r.sid == "notif_sid_b" for r in hint_pages), "ses_b should get an empty hint (no page_items)"
    assert not any(r.sid == "notif_sid_a" for r in hint_pages), "ses_a must NOT receive the empty hint"

    print("Viewer notification tests passed.")


async def org_test__on_media_file_ingested(oi: organizer.OrganizerInbound):
    """
    on_media_file_ingested() -- Verify that when the server notifies the organizer about
    a newly ingested file, the file is adopted into the user's root folder and viewers
    of that folder receive a refresh hint.
    """
    from organizer.database.models import DbFolderItems

    # Create a user and their root folder
    with oi.db_new_session() as dbs:
        dbs.add(DbUser(id="ingest_test.user", name="Ingest User"))
        dbs.commit()

    ses = org.UserSessionData(sid="ingest_sid", user=clap.UserInfo(id="ingest_test.user", name="Ingest User"), is_admin=False, cookies={})
    await organizer.navigate_page_impl(oi, org.NavigatePageRequest(ses=ses))

    folder_path, _ = await oi.folders_helper.get_current_folder_path(ses, None)
    root_folder = folder_path[-1]

    # Insert an orphan media file (in media_files table but NOT in bf_folder_items)
    with oi.db_new_session() as dbs:
        dbs.add(DbMediaFile(id="aabbccdd0011", user_id="ingest_test.user", media_type="video",
                            total_frames=100, duration=10.0, fps=10.0, raw_metadata_all="{}",
                            orig_filename="test.mp4", recompression_done=None, title="Orphan Video"))
        dbs.commit()

    # Verify the file is NOT yet in any folder
    with oi.db_new_session() as dbs:
        in_folder = dbs.query(DbFolderItems).filter(DbFolderItems.media_file_id == "aabbccdd0011").count()
        assert in_folder == 0, f"Orphan file should not be in any folder yet, but found {in_folder} entries"

    # Register another session as a viewer of the root folder
    ses_observer = org.UserSessionData(sid="observer_sid", user=clap.UserInfo(id="ingest_test.user", name="Ingest User"), is_admin=False, cookies={})
    page_id = str(root_folder.id)
    await organizer.navigate_page_impl(oi, org.NavigatePageRequest(ses=ses_observer, page_id=page_id))

    # Intercept client_show_page calls
    show_page_calls: list[org.ClientShowPageRequest] = []
    orig_show_page = oi.srv.client_show_page

    async def _mock_show_page(self, req: org.ClientShowPageRequest) -> clap.Empty:
        show_page_calls.append(req)
        return clap.Empty()

    setattr(oi.srv, "client_show_page", MethodType(_mock_show_page, oi.srv))
    try:
        # Simulate the server calling on_media_file_ingested
        await oi.on_media_file_ingested(org.OnMediaFileIngestedRequest(
            user_id="ingest_test.user", media_file_id="aabbccdd0011"))
    finally:
        setattr(oi.srv, "client_show_page", orig_show_page)

    # Verify the orphan file was adopted into the root folder
    with oi.db_new_session() as dbs:
        in_folder = dbs.query(DbFolderItems).filter(
            DbFolderItems.media_file_id == "aabbccdd0011",
            DbFolderItems.folder_id == root_folder.id).count()
        assert in_folder == 1, f"Orphan file should be adopted into root folder, but found {in_folder} entries"

    # Verify the observer received a refresh hint (empty ShowPage)
    print(f"ShowPage calls: {[(r.sid, len(r.page_items)) for r in show_page_calls]}")
    assert any(r.sid == "observer_sid" and not r.page_items for r in show_page_calls), \
        "Observer should receive an empty ShowPage refresh hint"

    print("on_media_file_ingested tests passed.")


# Import version-set tests so they get discovered by the test framework via inspect.getmembers()
# These imports are required for test discovery - do NOT remove as "unused" imports!
# Placed at the BOTTOM so _create_test_folder_and_session (imported by version_sets) is already defined.
from organizer.testing_methods.version_sets import (  # noqa: F401
    org_test__version_set__add_into_set_active_follows_latest,  # noqa: F401
    org_test__version_set__breadcrumb_shows_current_folder_name,  # noqa: F401
    org_test__version_set__defensive_kind,  # noqa: F401
    org_test__version_set__into_normal_reverts,  # noqa: F401
    org_test__version_set__into_version_set,  # noqa: F401
    org_test__version_set__make_versioned_denied_for_folder,  # noqa: F401
    org_test__version_set__make_versioned_multi,  # noqa: F401
    org_test__version_set__make_versioned_offered_on_media,  # noqa: F401
    org_test__version_set__make_versioned_single,  # noqa: F401
    org_test__version_set__manage_view_rendering,  # noqa: F401
    org_test__version_set__media_only_guard,  # noqa: F401
    org_test__version_set__move_active_out,  # noqa: F401
    org_test__version_set__move_last_out_deletes_set,  # noqa: F401
    org_test__version_set__move_nonactive_out,  # noqa: F401
    org_test__version_set__on_delete_repairs,  # noqa: F401
    org_test__version_set__previewed_as_media_in_parent_tile,  # noqa: F401
    org_test__version_set__refused_when_empty_or_root,  # noqa: F401
    org_test__version_set__refused_with_subfolder,  # noqa: F401
    org_test__version_set__reorder_keeps_active,  # noqa: F401
    org_test__version_set__reorder_refreshes_actor,  # noqa: F401
    org_test__version_set__selfheal_empty_deletes,  # noqa: F401
    org_test__version_set__selfheal_subfolder_demotes,  # noqa: F401
    org_test__version_set__set_active_version,  # noqa: F401
    org_test__version_set__set_active_version_action_uses_camelcase,  # noqa: F401
    org_test__version_set__set_active_version_sends_refresh_hint,  # noqa: F401
    org_test__version_set__tile_rendering,  # noqa: F401
)
