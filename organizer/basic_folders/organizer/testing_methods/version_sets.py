from __future__ import annotations

import json
import uuid
from typing import Optional

import sqlalchemy

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus
import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org

import organizer
from organizer.config import PATH_COOKIE_NAME
from organizer.database.models import DbFolder, DbFolderItems, DbMediaFile, FolderKind
from organizer.testing_methods import _create_test_folder_and_session


# End-to-end version-set tests: exercised through cmd_from_client / move_to_folder / reorder_items + construct_navi_page, asserting observable DB + rendered-page state.


async def _create_test_media_in_folder(oi: organizer.OrganizerInbound, ses: org.UserSessionData, folder: DbFolder, count: int) -> list[str]:
    """
    Insert `count` media files owned by ses.user into `folder` (sort_order 0..count-1).
    Returns their ids ordered leftmost-first (matching fetch_folder_contents order).
    """
    ids: list[str] = []
    with oi.db_new_session() as dbs:
        with dbs.begin_nested():
            for i in range(count):
                mid = uuid.uuid4().hex  # hex-only id (listing renderer asserts ^[0-9a-fA-F]+$)
                dbs.execute(sqlalchemy.text(
                    "INSERT INTO media_files (id, user_id, media_type, title, has_thumbnail, thumb_sheet_cols, thumb_sheet_rows) "
                    "VALUES (:id, :uid, 'video', :title, 1, 4, 4)"),
                    {"id": mid, "uid": ses.user.id, "title": f"Test media {i}"})
                dbs.add(DbFolderItems(folder_id=folder.id, media_file_id=mid, sort_order=i))
                ids.append(mid)
    return ids


async def org_test__version_set__into_version_set(oi: organizer.OrganizerInbound):
    """
    set_folder_kind(version_set) marks a non-empty folder and sets active = leftmost.
    """
    ses, root_fld = await _create_test_folder_and_session(oi)
    fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Version Set")
    media_ids = await _create_test_media_in_folder(oi, ses, fld, 2)

    leftmost = (await oi.folders_helper.fetch_folder_contents(fld, ses))[0].id
    assert leftmost == media_ids[0], f"Expected leftmost {media_ids[0]}, got {leftmost}"

    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="set_folder_kind",
        args=json.dumps({"id": fld.id, "kind": "version_set"})))

    with oi.db_new_session() as dbs:
        got = dbs.query(DbFolder).filter(DbFolder.id == fld.id).one()
        assert got.kind == FolderKind.VERSION_SET.value, f"kind should be version_set, got {got.kind!r}"
        assert got.active_media_file_id == leftmost, f"active should be {leftmost}, got {got.active_media_file_id!r}"
    print("into_version_set OK")


async def org_test__version_set__refused_with_subfolder(oi: organizer.OrganizerInbound):
    """Into Version Set is refused (error shown, kind unchanged) when the folder contains a subfolder."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Has Subfolder")
    await oi.folders_helper.create_folder(oi.db_new_session(), ses, fld, "Sub")
    await _create_test_media_in_folder(oi, ses, fld, 1)  # non-empty, but also has a subfolder

    shown: list = []
    orig_show = oi.srv.client_show_user_message
    async def mock_show(req: org.ClientShowUserMessageRequest) -> clap.Empty:
        if req.msg:
            shown.append(req.msg)
        return await orig_show(req)
    setattr(oi.srv, "client_show_user_message", mock_show)
    try:
        ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])
        await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
            cmd="set_folder_kind", args=json.dumps({"id": fld.id, "kind": "version_set"})))
    finally:
        setattr(oi.srv, "client_show_user_message", orig_show)

    with oi.db_new_session() as dbs:
        got = dbs.query(DbFolder).filter(DbFolder.id == fld.id).one()
        assert got.kind == FolderKind.NORMAL.value, f"kind should stay normal, got {got.kind!r}"
        assert got.active_media_file_id is None
    assert any(m.type == clap.UserMessageType.ERROR for m in shown), "Expected an error message about the subfolder"
    print("refused_with_subfolder OK")


async def org_test__version_set__refused_when_empty_or_root(oi: organizer.OrganizerInbound):
    """Into Version Set refused for an empty folder and for the root folder (kind unchanged)."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    empty_fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Empty")
    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])

    for target_id in (empty_fld.id, root_fld.id):
        await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
            cmd="set_folder_kind", args=json.dumps({"id": target_id, "kind": "version_set"})))
        with oi.db_new_session() as dbs:
            got = dbs.query(DbFolder).filter(DbFolder.id == target_id).one()
            assert got.kind == FolderKind.NORMAL.value, f"folder {target_id} kind should stay normal, got {got.kind!r}"
    print("refused_when_empty_or_root OK")


async def org_test__version_set__into_normal_reverts(oi: organizer.OrganizerInbound):
    """Into Normal Folder reverts a version set: kind normal, active cleared, contents + order unchanged."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Version Set")
    await _create_test_media_in_folder(oi, ses, fld, 3)
    order_before = [it.id for it in await oi.folders_helper.fetch_folder_contents(fld, ses)]
    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])

    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="set_folder_kind", args=json.dumps({"id": fld.id, "kind": "version_set"})))
    with oi.db_new_session() as dbs:
        got = dbs.query(DbFolder).filter(DbFolder.id == fld.id).one()
        assert got.kind == FolderKind.VERSION_SET.value
        assert got.active_media_file_id is not None

    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="set_folder_kind", args=json.dumps({"id": fld.id, "kind": "normal"})))
    with oi.db_new_session() as dbs:
        got = dbs.query(DbFolder).filter(DbFolder.id == fld.id).one()
        assert got.kind == FolderKind.NORMAL.value, f"kind should be normal, got {got.kind!r}"
        assert got.active_media_file_id is None, f"active should be cleared, got {got.active_media_file_id!r}"

    order_after = [it.id for it in await oi.folders_helper.fetch_folder_contents(fld, ses)]
    assert order_after == order_before, f"order changed: {order_before} -> {order_after}"
    print("into_normal_reverts OK")


def _version_sets_in(contents: list) -> list[DbFolder]:
    return [f for f in contents if isinstance(f, DbFolder) and f.kind == FolderKind.VERSION_SET.value]


async def org_test__version_set__make_versioned_single(oi: organizer.OrganizerInbound):
    """Make Versioned on a single media file creates a version_set folder (titled after the file) in the same parent."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    media_ids = await _create_test_media_in_folder(oi, ses, root_fld, 1)
    mid = media_ids[0]

    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="make_versioned", args=json.dumps({"ids": [{"mediaFileId": mid}]})))

    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    vsets = _version_sets_in(root_cont)
    assert len(vsets) == 1, f"expected exactly one version set in parent, got {len(vsets)}"
    vset = vsets[0]
    assert vset.title == "Test media 0", f"version set should be named after the file, got {vset.title!r}"
    assert vset.active_media_file_id == mid, f"active should be the file, got {vset.active_media_file_id!r}"
    assert mid not in [c.id for c in root_cont], "file should no longer be directly in parent"
    vset_cont = [c.id for c in await oi.folders_helper.fetch_folder_contents(vset, ses)]
    assert vset_cont == [mid], f"version set should contain the file, got {vset_cont}"
    print("make_versioned_single OK")


async def org_test__version_set__make_versioned_multi(oi: organizer.OrganizerInbound):
    """Make Versioned on several media files groups them into one set, display order preserved, active = leftmost."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    media_ids = await _create_test_media_in_folder(oi, ses, root_fld, 3)  # leftmost-first: [m0, m1, m2]

    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="make_versioned", args=json.dumps({"ids": [{"mediaFileId": m} for m in media_ids]})))

    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    vsets = _version_sets_in(root_cont)
    assert len(vsets) == 1, f"expected one version set, got {len(vsets)}"
    vset = vsets[0]
    assert vset.title == "Test media 0", f"named after the first file, got {vset.title!r}"
    assert vset.active_media_file_id == media_ids[0], f"active should be leftmost, got {vset.active_media_file_id!r}"
    vset_cont = [c.id for c in await oi.folders_helper.fetch_folder_contents(vset, ses)]
    assert vset_cont == media_ids, f"display order should be preserved, expected {media_ids}, got {vset_cont}"
    print("make_versioned_multi OK")


async def org_test__version_set__make_versioned_denied_for_folder(oi: organizer.OrganizerInbound):
    """Make Versioned is refused (error shown, nothing created) if the selection contains a folder."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    media_ids = await _create_test_media_in_folder(oi, ses, root_fld, 1)
    sub = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "A Folder")

    shown: list = []
    orig_show = oi.srv.client_show_user_message
    async def mock_show(req: org.ClientShowUserMessageRequest) -> clap.Empty:
        if req.msg:
            shown.append(req.msg)
        return await orig_show(req)
    setattr(oi.srv, "client_show_user_message", mock_show)
    try:
        ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])
        await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
            cmd="make_versioned",
            args=json.dumps({"ids": [{"mediaFileId": media_ids[0]}, {"folderId": str(sub.id)}]})))
    finally:
        setattr(oi.srv, "client_show_user_message", orig_show)

    root_cont = await oi.folders_helper.fetch_folder_contents(root_fld, ses)
    assert len(_version_sets_in(root_cont)) == 0, "no version set should have been created"
    assert any(c.id == media_ids[0] for c in root_cont), "media file should still be in the parent"
    assert any(m.type == clap.UserMessageType.ERROR for m in shown), "expected an error message"
    print("make_versioned_denied_for_folder OK")


def _listing_items(page: org.ClientShowPageRequest) -> list[clap.PageItemFolderListingItem]:
    return [it for pi in page.page_items if pi.folder_listing for it in pi.folder_listing.items]


def _badge_text(it: clap.PageItemFolderListingItem) -> str:
    assert it.vis and it.vis.badges, "item is missing a version badge"
    return it.vis.badges[0].text


def _first_folder_listing(page: org.ClientShowPageRequest) -> clap.PageItemFolderListing | None:
    for pi in page.page_items:
        if pi.folder_listing:
            return pi.folder_listing
    return None


async def _make_version_set(oi: organizer.OrganizerInbound, ses: org.UserSessionData, parent: DbFolder, n: int) -> tuple[DbFolder, list[str]]:
    """Create a subfolder of `parent` with n media files and convert it to a version set. Returns (folder, media_ids)."""
    fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, parent, f"VSet {uuid.uuid4().hex[:6]}")
    media_ids = await _create_test_media_in_folder(oi, ses, fld, n)
    ses.cookies[PATH_COOKIE_NAME] = json.dumps([parent.id])
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="set_folder_kind", args=json.dumps({"id": fld.id, "kind": "version_set"})))
    return fld, media_ids


async def org_test__version_set__tile_rendering(oi: organizer.OrganizerInbound):
    """The version-set tile renders as the active MediaFile + orange badge + an openMediaFile(headerHtml=<select>) open-action; popup has manage/normal, no new_folder."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 3)  # active = leftmost = media_ids[0]

    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id]))
    vset_items = [it for it in _listing_items(page)
                  if it.folder and it.folder.preview_as_media_tile]
    assert len(vset_items) == 1, f"expected one media-tile (version-set) folder, got {len(vset_items)}"
    it = vset_items[0]

    # preview_items[0] is the active MediaFile, including preview_data (thumb sheet) for scrubbing
    assert len(it.folder.preview_items) == 1, "version-set tile should have one preview item (the active version)"
    pv = it.folder.preview_items[0]
    assert pv.media_file and pv.media_file.id == media_ids[0], "preview should be the active media file"
    assert pv.media_file.preview_data and pv.media_file.preview_data.thumb_sheet, "active media file should carry preview_data/thumb_sheet"

    # active is the latest (leftmost) of 3 -> "v3"
    assert it.vis and len(it.vis.badges) == 1, "expected one badge"
    assert it.vis.badges[0].text == "v3", f"badge should be v3, got {it.vis.badges[0].text!r}"

    # open-action: opens the active version with a player-header <select> for switching versions
    code = it.open_action.code if it.open_action else ""
    assert "clapshot.openMediaFile(" in code, "open_action should call openMediaFile"
    assert "headerHtml" in code and "<select" in code, "open_action should pass a <select> as headerHtml"
    assert "set_active_version" in code, "the header <select> should switch the active version"
    assert media_ids[0] in code, "open_action should open the active version"
    assert "openVersionSet" not in code, "openVersionSet must be gone"
    assert "manage_versions" in it.popup_actions, "popup should include manage_versions"
    assert "into_normal_folder" in it.popup_actions, "popup should include into_normal_folder"
    assert "new_folder" not in it.popup_actions, "popup must not include new_folder"
    print("tile_rendering OK")


async def org_test__version_set__manage_view_rendering(oi: organizer.OrganizerInbound):
    """Inside a version set: each version has a vN badge, the active one is cyan + has set_active_version, no new_folder."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 3)  # active = leftmost = media_ids[0]

    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id, vset.id]))
    listing = _first_folder_listing(page)
    assert listing is not None, "expected a folder listing"
    assert "new_folder" not in listing.popup_actions, "version set must not offer New folder"
    assert listing.allow_reordering, "reordering should be allowed"
    assert listing.allow_upload, "upload should be allowed"

    media_items = [it for it in listing.items if it.media_file]
    assert len(media_items) == 3, f"expected 3 versions, got {len(media_items)}"
    by_id = {it.media_file.id: it for it in media_items}
    # leftmost = latest: media_ids[0]=v3, [1]=v2, [2]=v1
    assert _badge_text(by_id[media_ids[0]]) == "v3"
    assert _badge_text(by_id[media_ids[1]]) == "v2"
    assert _badge_text(by_id[media_ids[2]]) == "v1"

    active_item = by_id[media_ids[0]]
    assert active_item.vis and active_item.vis.base_color is not None, "active version should be tinted (cyan)"
    assert "set_active_version" in active_item.popup_actions
    non_active = by_id[media_ids[1]]
    assert non_active.vis and non_active.vis.base_color is None, "non-active version should not be tinted"
    assert "set_active_version" in non_active.popup_actions
    print("manage_view_rendering OK")


async def org_test__version_set__set_active_version(oi: organizer.OrganizerInbound):
    """set_active_version changes the active id; the parent tile badge becomes 'vN of M' when not the latest."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 3)  # active = media_ids[0] (v3, latest)

    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="set_active_version",
        args=json.dumps({"folder_id": vset.id, "media_file_id": media_ids[1]})))

    with oi.db_new_session() as dbs:
        got = dbs.query(DbFolder).filter(DbFolder.id == vset.id).one()
        assert got.active_media_file_id == media_ids[1], f"active should be media_ids[1], got {got.active_media_file_id!r}"

    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id]))
    vit = [it for it in _listing_items(page)
           if it.folder and it.folder.preview_as_media_tile][0]
    assert _badge_text(vit) == "v2 of 3", f"badge should be 'v2 of 3', got {_badge_text(vit)!r}"
    print("set_active_version OK")


async def _active_of(oi: organizer.OrganizerInbound, folder_id: int) -> Optional[str]:
    with oi.db_new_session() as dbs:
        return dbs.query(DbFolder).filter(DbFolder.id == folder_id).one().active_media_file_id


async def org_test__version_set__add_into_set_active_follows_latest(oi: organizer.OrganizerInbound):
    """Adding into a set front-inserts (new latest); active follows the new latest only if it was the previous latest."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 2)  # [m0, m1], active = m0 (latest)
    extra_ids = await _create_test_media_in_folder(oi, ses, root_fld, 2)  # e0, e1 sitting in root

    # Sub-case A: active was the latest -> active follows the newly added latest.
    await oi.move_to_folder(org.MoveToFolderRequest(
        ses, ids=[clap.FolderItemId(media_file_id=extra_ids[0])],
        dst_folder_id=str(vset.id), listing_data={"folder_id": str(root_fld.id)}))
    contents = [c.id for c in await oi.folders_helper.fetch_folder_contents(vset, ses)]
    assert contents[0] == extra_ids[0], f"added file should be leftmost, got {contents}"
    assert await _active_of(oi, vset.id) == extra_ids[0], "active should follow the new latest"

    # Sub-case B: active is NOT the latest -> adding a newer file leaves active unchanged.
    await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
        cmd="set_active_version", args=json.dumps({"folder_id": vset.id, "media_file_id": media_ids[0]})))
    await oi.move_to_folder(org.MoveToFolderRequest(
        ses, ids=[clap.FolderItemId(media_file_id=extra_ids[1])],
        dst_folder_id=str(vset.id), listing_data={"folder_id": str(root_fld.id)}))
    assert await _active_of(oi, vset.id) == media_ids[0], "active should stay put when it wasn't the latest"
    print("add_into_set_active_follows_latest OK")


async def org_test__version_set__move_nonactive_out(oi: organizer.OrganizerInbound):
    """Moving a non-active version out of a set leaves the active pointer unchanged."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 3)  # active = m0

    await oi.move_to_folder(org.MoveToFolderRequest(
        ses, ids=[clap.FolderItemId(media_file_id=media_ids[2])],
        dst_folder_id=str(root_fld.id), listing_data={"folder_id": str(vset.id)}))

    assert await _active_of(oi, vset.id) == media_ids[0], "active should be unchanged"
    vcont = {c.id for c in await oi.folders_helper.fetch_folder_contents(vset, ses)}
    assert vcont == {media_ids[0], media_ids[1]}, f"moved-out file should be gone, got {vcont}"
    rcont = {c.id for c in await oi.folders_helper.fetch_folder_contents(root_fld, ses)}
    assert media_ids[2] in rcont, "moved-out file should be in the parent"
    print("move_nonactive_out OK")


async def org_test__version_set__move_active_out(oi: organizer.OrganizerInbound):
    """Moving the active version out makes the new leftmost the active one."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 3)  # active = m0 (leftmost)

    await oi.move_to_folder(org.MoveToFolderRequest(
        ses, ids=[clap.FolderItemId(media_file_id=media_ids[0])],
        dst_folder_id=str(root_fld.id), listing_data={"folder_id": str(vset.id)}))

    assert await _active_of(oi, vset.id) == media_ids[1], "active should fall back to the new leftmost"
    vcont = [c.id for c in await oi.folders_helper.fetch_folder_contents(vset, ses)]
    assert vcont == [media_ids[1], media_ids[2]], f"got {vcont}"
    print("move_active_out OK")


async def org_test__version_set__move_last_out_deletes_set(oi: organizer.OrganizerInbound):
    """Moving the last item out of a set deletes the (now empty) set and redirects to the parent folder."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    parent = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Parent")
    vset, media_ids = await _make_version_set(oi, ses, parent, 1)  # single-version set in "Parent"

    captured: list[org.ClientShowPageRequest] = []
    orig_show_page = oi.srv.client_show_page
    async def mock_show_page(req: org.ClientShowPageRequest) -> clap.Empty:
        captured.append(req)
        return await orig_show_page(req)
    setattr(oi.srv, "client_show_page", mock_show_page)
    try:
        ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id, parent.id, vset.id])  # viewing inside the set
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses, ids=[clap.FolderItemId(media_file_id=media_ids[0])],
            dst_folder_id=str(parent.id), listing_data={"folder_id": str(vset.id)}))
    finally:
        setattr(oi.srv, "client_show_page", orig_show_page)

    with oi.db_new_session() as dbs:
        assert dbs.query(DbFolder).filter(DbFolder.id == vset.id).one_or_none() is None, "empty set should be deleted"
    pcont = {c.id for c in await oi.folders_helper.fetch_folder_contents(parent, ses)}
    assert media_ids[0] in pcont, "the moved file should now be in the parent"
    pages = [p for p in captured if p.page_items]
    assert pages and pages[-1].page_title == "Parent", f"should redirect to parent, got {[p.page_title for p in pages]}"
    print("move_last_out_deletes_set OK")


async def org_test__version_set__media_only_guard(oi: organizer.OrganizerInbound):
    """Moving a folder / version set into a version set is refused, the set is unchanged, AND the client's
    view is refreshed so the (optimistically drag-removed) folder reappears."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 2)
    other = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Other Folder")
    ses.cookies[PATH_COOKIE_NAME] = json.dumps([root_fld.id])  # the client is viewing root

    captured: list[org.ClientShowPageRequest] = []
    orig = oi.srv.client_show_page
    async def mock_show_page(req: org.ClientShowPageRequest) -> clap.Empty:
        captured.append(req)
        return await orig(req)
    setattr(oi.srv, "client_show_page", mock_show_page)
    try:
        await oi.move_to_folder(org.MoveToFolderRequest(
            ses, ids=[clap.FolderItemId(folder_id=str(other.id))],
            dst_folder_id=str(vset.id), listing_data={"folder_id": str(root_fld.id)}))
        assert False, "expected GRPCError"
    except GRPCError as e:
        assert e.status == GrpcStatus.INVALID_ARGUMENT, f"expected INVALID_ARGUMENT, got {e.status}"
    finally:
        setattr(oi.srv, "client_show_page", orig)

    vcont = await oi.folders_helper.fetch_folder_contents(vset, ses)
    assert {c.id for c in vcont} == set(media_ids), "version set contents should be unchanged"
    assert all(isinstance(c, DbMediaFile) for c in vcont), "version set should still contain only media"

    # The rejected drop must refresh the client's view so the un-moved folder reappears.
    assert captured, "a refresh page should be pushed to the client after a rejected move"
    refreshed = _listing_items(captured[-1])
    assert any(it.folder and it.folder.id == str(other.id) for it in refreshed), \
        "the refreshed view should still contain the folder that was not moved"
    print("media_only_guard OK")


async def org_test__version_set__selfheal_subfolder_demotes(oi: organizer.OrganizerInbound):
    """A version set that (via direct DB state) gains a subfolder renders defensively as a normal folder
    WITHOUT the render mutating the DB; repair_version_sets() then demotes it (kind normal, active cleared)."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 2)

    # Sneak a subfolder into the set, bypassing the normal guards.
    with oi.db_new_session() as dbs:
        with dbs.begin_nested():
            sub = DbFolder(user_id=ses.user.id, title="Sneaky Sub")
            dbs.add(sub)
            dbs.flush()
            dbs.add(DbFolderItems(folder_id=vset.id, subfolder_id=sub.id, sort_order=99))

    # Render is read-only: the invalid set shows as a normal folder, but the DB is NOT changed by render.
    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id]))
    fitem = [it for it in _listing_items(page) if it.folder and it.folder.id == str(vset.id)][0]
    assert not fitem.folder.preview_as_media_tile, "invalid set should render as a normal folder"
    assert not (fitem.vis and fitem.vis.badges), "invalid set should have no version badge"
    with oi.db_new_session() as dbs:
        assert dbs.query(DbFolder).filter(DbFolder.id == vset.id).one().kind == FolderKind.VERSION_SET.value, \
            "render must not mutate the DB (still version_set until housekeeping runs)"

    # Housekeeping demotes it.
    await oi.folders_helper.repair_version_sets(ses.user.id)
    with oi.db_new_session() as dbs:
        got = dbs.query(DbFolder).filter(DbFolder.id == vset.id).one()
        assert got.kind == FolderKind.NORMAL.value, f"should be demoted to normal, got {got.kind!r}"
        assert got.active_media_file_id is None, "active should be cleared after demotion"
    print("selfheal_subfolder_demotes OK")


async def org_test__version_set__selfheal_empty_deletes(oi: organizer.OrganizerInbound):
    """An empty version set (direct DB state) renders defensively (read-only, not as a version-set tile,
    no crash) and is deleted by repair_version_sets()."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 1)

    # Empty the set directly (re-home its only media into the parent), bypassing move-out cleanup.
    with oi.db_new_session() as dbs:
        with dbs.begin_nested():
            dbs.query(DbFolderItems).filter(DbFolderItems.media_file_id == media_ids[0]).update(
                {"folder_id": root_fld.id, "sort_order": 0})

    # Render is read-only: it must not crash and must not delete the folder.
    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id]))
    rendered = [it for it in _listing_items(page) if it.folder and it.folder.id == str(vset.id)]
    assert rendered and not rendered[0].folder.preview_as_media_tile, "empty set should render as a normal folder"
    with oi.db_new_session() as dbs:
        assert dbs.query(DbFolder).filter(DbFolder.id == vset.id).one_or_none() is not None, \
            "render must not delete the folder (housekeeping does)"

    # Housekeeping deletes the now-empty set.
    await oi.folders_helper.repair_version_sets(ses.user.id)
    with oi.db_new_session() as dbs:
        assert dbs.query(DbFolder).filter(DbFolder.id == vset.id).one_or_none() is None, "empty set should be deleted"
    print("selfheal_empty_deletes OK")


async def org_test__version_set__on_delete_repairs(oi: organizer.OrganizerInbound):
    """on_media_file_deleted repairs version sets degraded by a media deletion: stale active resets to the
    new leftmost; a set emptied by the deletion is removed. (Simulates the server's FK cascade.)"""
    ses, root_fld = await _create_test_folder_and_session(oi)

    # Case A: the active (leftmost) member is deleted -> active resets to the new leftmost.
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 3)  # active = media_ids[0]
    with oi.db_new_session() as dbs:  # mimic the server delete: cascade removes the link + nulls active
        with dbs.begin_nested():
            dbs.query(DbFolderItems).filter(DbFolderItems.media_file_id == media_ids[0]).delete()
            dbs.query(DbFolder).filter(DbFolder.id == vset.id).update({"active_media_file_id": None})
    await oi.on_media_file_deleted(org.OnMediaFileDeletedRequest(user_id=ses.user.id, media_file_id=media_ids[0]))
    assert await _active_of(oi, vset.id) == media_ids[1], "active should reset to the new leftmost"

    # Case B: the last member is deleted -> the now-empty set is removed.
    solo, solo_ids = await _make_version_set(oi, ses, root_fld, 1)
    with oi.db_new_session() as dbs:
        with dbs.begin_nested():
            dbs.query(DbFolderItems).filter(DbFolderItems.media_file_id == solo_ids[0]).delete()
            dbs.query(DbFolder).filter(DbFolder.id == solo.id).update({"active_media_file_id": None})
    await oi.on_media_file_deleted(org.OnMediaFileDeletedRequest(user_id=ses.user.id, media_file_id=solo_ids[0]))
    with oi.db_new_session() as dbs:
        assert dbs.query(DbFolder).filter(DbFolder.id == solo.id).one_or_none() is None, "emptied set should be deleted"
    print("on_delete_repairs OK")


async def org_test__version_set__set_active_version_sends_refresh_hint(oi: organizer.OrganizerInbound):
    """set_active_version sends only an empty ShowPage refresh hint (never a full page) to viewers, so it
    can't close the player when invoked from the in-player version <select>. (#1)"""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 3)

    # Register the acting session as a viewer of the set (as navigating into it would).
    await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id, vset.id]))

    captured: list[org.ClientShowPageRequest] = []
    orig = oi.srv.client_show_page
    async def mock_show_page(req: org.ClientShowPageRequest) -> clap.Empty:
        captured.append(req)
        return await orig(req)
    setattr(oi.srv, "client_show_page", mock_show_page)
    try:
        await oi.cmd_from_client(org.CmdFromClientRequest(ses=ses,
            cmd="set_active_version", args=json.dumps({"folder_id": vset.id, "media_file_id": media_ids[1]})))
    finally:
        setattr(oi.srv, "client_show_page", orig)

    assert captured, "expected a refresh hint to the set's viewers"
    assert all(not req.page_items for req in captured), \
        "set_active_version must send only empty refresh hints, never a full page (a full page closes the player)"
    with oi.db_new_session() as dbs:
        assert dbs.query(DbFolder).filter(DbFolder.id == vset.id).one().active_media_file_id == media_ids[1]
    print("set_active_version_sends_refresh_hint OK")


async def org_test__version_set__set_active_version_action_uses_camelcase(oi: organizer.OrganizerInbound):
    """The 'Set Active Version' popup action reads the camelCase proto field (it.mediaFile), not snake_case. (#2)"""
    action = oi.actions_helper.make_set_active_version_action().action
    assert action is not None, "set_active_version action must carry a ScriptCall"
    code = action.code
    assert "it?.mediaFile?.id" in code, "action must read it.mediaFile (camelCase proto field)"
    assert "media_file?.id" not in code, "action must not read snake_case it.media_file"
    print("set_active_version_action_uses_camelcase OK")


async def org_test__version_set__make_versioned_offered_on_media(oi: organizer.OrganizerInbound):
    """Media tiles in a normal folder offer 'make_versioned'; versions inside a set don't (they offer
    set_active_version instead). Guards the popup-wiring that was missing."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    await _create_test_media_in_folder(oi, ses, root_fld, 2)

    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id]))
    media_items = [it for it in _listing_items(page) if it.media_file]
    assert media_items, "expected media items in the folder"
    for it in media_items:
        assert "make_versioned" in it.popup_actions, f"media tile should offer make_versioned, got {it.popup_actions}"

    vset, _ = await _make_version_set(oi, ses, root_fld, 2)
    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id, vset.id]))
    versions = [it for it in _listing_items(page) if it.media_file]
    assert versions, "expected versions inside the set"
    for it in versions:
        assert "make_versioned" not in it.popup_actions, "versions inside a set must not offer make_versioned"
        assert "set_active_version" in it.popup_actions, "versions should offer set_active_version"
    print("make_versioned_offered_on_media OK")


async def org_test__version_set__previewed_as_media_in_parent_tile(oi: organizer.OrganizerInbound):
    """A version set nested in a normal folder is previewed (in that folder's tile) by its ACTIVE
    version's media thumbnail, not as a folder mini-tile."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    parent = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Parent")
    vset, vids = await _make_version_set(oi, ses, parent, 2)  # version set inside "Parent"; active = vids[0]

    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id]))
    parent_item = [it for it in _listing_items(page) if it.folder and it.folder.id == str(parent.id)][0]
    pv = parent_item.folder.preview_items
    assert any(p.media_file and p.media_file.id == vids[0] for p in pv), \
        f"version set should preview as its active media {vids[0]}, got {[(p.media_file.id if p.media_file else 'folder:'+p.folder.id) for p in pv]}"
    assert not any(p.folder and p.folder.id == str(vset.id) for p in pv), \
        "version set must not appear as a folder mini-tile in the parent's preview"
    print("previewed_as_media_in_parent_tile OK")


async def org_test__version_set__breadcrumb_shows_current_folder_name(oi: organizer.OrganizerInbound):
    """The terminal (current) breadcrumb crumb is the folder's own name, not 'Home'; only the user's
    root crumb is 'Home'. Regression: get_current_folder_path's 2nd value is the root, not the current
    folder, so the breadcrumb's Home check no longer matches the current folder."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    sub = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "SomeFolder")
    await _create_test_media_in_folder(oi, ses, sub, 1)

    # Normal subfolder: "Home ▶ **SomeFolder**".
    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id, sub.id]))
    bc = page.page_items[0].html
    assert "<strong>SomeFolder</strong>" in bc, f"terminal crumb should be the folder name, got: {bc}"
    assert "<strong>Home</strong>" not in bc, f"terminal crumb must not be 'Home', got: {bc}"
    assert ">Home</a>" in bc, f"the root crumb should be a clickable 'Home' link, got: {bc}"

    # Version-set manage view: terminal crumb is the set's title.
    vset, _ = await _make_version_set(oi, ses, root_fld, 2)
    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id, vset.id]))
    bc = page.page_items[0].html
    assert f"<strong>{vset.title}</strong>" in bc, f"version-set terminal crumb should be the set title, got: {bc}"
    assert "<strong>Home</strong>" not in bc, f"terminal crumb must not be 'Home', got: {bc}"
    print("breadcrumb_shows_current_folder_name OK")


async def org_test__version_set__reorder_refreshes_actor(oi: organizer.OrganizerInbound):
    """A version-set reorder refreshes the acting client too (so vN badges renumber inline); a normal
    folder reorder does NOT refresh the actor (the drag already moved the tiles client-side)."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, vids = await _make_version_set(oi, ses, root_fld, 3)
    nf = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Normal")
    nids = await _create_test_media_in_folder(oi, ses, nf, 3)

    captured: list[str] = []
    orig = oi.srv.client_show_page
    async def mock_show_page(req: org.ClientShowPageRequest) -> clap.Empty:
        captured.append(req.sid)
        return await orig(req)
    setattr(oi.srv, "client_show_page", mock_show_page)
    try:
        # Version set: the actor IS refreshed (registered as a viewer via construct_navi_page).
        await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id, vset.id]))
        captured.clear()
        await oi.reorder_items(org.ReorderItemsRequest(
            ses, ids=[clap.FolderItemId(media_file_id=m) for m in [vids[2], vids[0], vids[1]]],
            listing_data={"folder_id": str(vset.id)}))
        assert ses.sid in captured, "version-set reorder must refresh the actor (to renumber vN badges)"

        # Normal folder: the actor is NOT refreshed.
        await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id, nf.id]))
        captured.clear()
        await oi.reorder_items(org.ReorderItemsRequest(
            ses, ids=[clap.FolderItemId(media_file_id=m) for m in [nids[2], nids[0], nids[1]]],
            listing_data={"folder_id": str(nf.id)}))
        assert ses.sid not in captured, "normal-folder reorder should not refresh the actor"
    finally:
        setattr(oi.srv, "client_show_page", orig)
    print("reorder_refreshes_actor OK")


async def org_test__version_set__reorder_keeps_active(oi: organizer.OrganizerInbound):
    """Reordering versions keeps the same active id; version badges recompute to the new positions."""
    ses, root_fld = await _create_test_folder_and_session(oi)
    vset, media_ids = await _make_version_set(oi, ses, root_fld, 3)  # [m0, m1, m2], active = m0

    new_order = [media_ids[2], media_ids[0], media_ids[1]]
    await oi.reorder_items(org.ReorderItemsRequest(
        ses, ids=[clap.FolderItemId(media_file_id=m) for m in new_order],
        listing_data={"folder_id": str(vset.id)}))

    assert await _active_of(oi, vset.id) == media_ids[0], "active id should be unchanged by reorder"
    contents = [c.id for c in await oi.folders_helper.fetch_folder_contents(vset, ses)]
    assert contents == new_order, f"order should be updated, got {contents}"

    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id, vset.id]))
    listing = _first_folder_listing(page)
    assert listing is not None
    by_id = {it.media_file.id: it for it in listing.items if it.media_file}
    assert _badge_text(by_id[media_ids[2]]) == "v3", "new leftmost is the latest"
    assert _badge_text(by_id[media_ids[0]]) == "v2"
    assert _badge_text(by_id[media_ids[1]]) == "v1"
    print("reorder_keeps_active OK")


async def org_test__version_set__defensive_kind(oi: organizer.OrganizerInbound):
    """Any kind that is not exactly 'version_set' (incl. None / unknown) is treated as a normal folder."""
    assert DbFolder(kind=None).is_version_set is False  # type: ignore[arg-type]
    assert DbFolder(kind="bogus").is_version_set is False
    assert DbFolder(kind="normal").is_version_set is False
    assert DbFolder(kind="version_set").is_version_set is True

    ses, root_fld = await _create_test_folder_and_session(oi)
    fld = await oi.folders_helper.create_folder(oi.db_new_session(), ses, root_fld, "Plain")
    await _create_test_media_in_folder(oi, ses, fld, 2)

    page = await oi.pages_helper.construct_navi_page(ses, json.dumps([root_fld.id]))
    fitem = [it for it in _listing_items(page) if it.folder and it.folder.id == str(fld.id)][0]
    assert not fitem.folder.preview_as_media_tile, "plain folder should render as a normal folder"
    assert not (fitem.vis and fitem.vis.badges), "plain folder should have no version badge"
    print("defensive_kind OK")
