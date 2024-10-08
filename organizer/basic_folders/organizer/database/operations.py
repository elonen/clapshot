from typing import Optional
import uuid

from textwrap import dedent
from logging import Logger

from typing import List, Tuple, Set

import sqlalchemy
from sqlalchemy import Engine, text as sqla_text
from sqlalchemy.orm import Session

import clapshot_grpc.proto.clapshot.organizer as org
import clapshot_grpc.proto.clapshot as clap

from clapshot_grpc.utilities import try_send_user_message

from .models import Base, DbFolder, DbFolderItems, DbSchemaMigrations, DbUser, DbMediaFile
from .migrations import ALL_MIGRATIONS

def db_check_pending_migrations(db: Engine) -> tuple[str, list[org.Migration]]:
    """
    Check the database for current schema version and return a list of migrations that have a higher version string.
    """
    with db.connect() as conn:
        # Create the schema migrations table if it doesn't exist
        Base.metadata.create_all(db, tables=[Base.metadata.tables[ DbSchemaMigrations.__tablename__]], checkfirst=True)
        # List migrations whose version string is higher than the current version
        res = None
        with Session(conn) as session:
            cur_ver: str = session.query(DbSchemaMigrations.version).order_by(DbSchemaMigrations.version.desc()).limit(1).scalar() or ''
            pending = [m.metadata for m in ALL_MIGRATIONS if m.metadata.version > cur_ver]
            return (cur_ver, pending)


def db_apply_migration(dbs: Session, migr_uuid: str, log: Logger):
    try:
        migration = next(m for m in ALL_MIGRATIONS if m.metadata.uuid == migr_uuid)
        assert migration.metadata.version, "Migration version must be set"
        db_set_pragma_foreign_keys(dbs, False, log)
        log.debug(f"Applying migration SQL...")
        with dbs.begin_nested():
            for sql in migration.up_sql.split(";"):
                dbs.execute(sqla_text(sql))
            dbs.add(DbSchemaMigrations(version=migration.metadata.version, migration_uuid=migration.metadata.uuid))
        db_set_pragma_foreign_keys(dbs, True, log)
    except StopIteration:
        raise ValueError(f"Migration with id '{migr_uuid}' not found")


def db_set_pragma_foreign_keys(dbs: Session, enable: bool, log: Logger):
    log.debug(f"Setting PRAGMA foreign_keys to {'ON' if enable else 'OFF'}")
    dbs.execute(sqla_text(f"PRAGMA foreign_keys = {'ON' if enable else 'OFF'}"))
    assert dbs.execute(sqla_text("PRAGMA foreign_keys")).scalar() == int(enable), "PRAGMA foreign_keys setting failed to change the FK state"


def db_test_orm_mappings(dbs: Session, log: Logger):
    """
    After migrations are applied, test the ORM mappings by inserting some data and checking that it can be read back.
    """
    log.debug("Testing ORM mappings...")
    with dbs.begin() as tx:
        rnd = uuid.uuid4().hex  # random string to avoid collisions
        test_video, test_user = f"video_{rnd}", f"user_{rnd}"

        dbs.execute(sqla_text("INSERT INTO users (id, name) VALUES (:id, :name)"), {"id": test_user, "name": f"Test user {rnd}"})
        dbs.execute(sqla_text("INSERT INTO media_files (id, user_id) VALUES (:id, :user_id)"), {"id": test_video, "user_id": test_user})
        dbs.flush()

        # Insert a parent folder
        parent_folder = DbFolder(user_id=test_user, title=f"Parent folder title {rnd}")
        dbs.add(parent_folder)
        dbs.flush() # make sure the parent_folder.id is set
        assert parent_folder.id is not None, "Parent folder.id was not set after insert"

        # Insert a child folder
        child_folder = DbFolder(user_id=test_user, title=f"Folder title {rnd}")
        dbs.add(child_folder)
        dbs.flush()
        assert child_folder.id is not None, "Folder.id was not set after insert"

        # Insert folder contents
        video_entry = DbFolderItems(media_file_id=test_video, folder_id=child_folder.id)
        dbs.add(video_entry)
        subfolder_entry = DbFolderItems(subfolder_id=child_folder.id, folder_id=parent_folder.id)
        dbs.add(subfolder_entry)
        dbs.flush()
        assert video_entry.folder_id == child_folder.id, "Video not in correct folder"
        assert subfolder_entry.folder_id == parent_folder.id, "Subfolder not in correct folder"

        tx.rollback()
        log.debug("ORM mappings test ok")


def db_check_and_fix_integrity(dbs: Session, log: Logger):
    """
    Check the database for some integrity issues and fix them.
    """
    log.debug("Checking database integrity...")
    with dbs.begin():
        dangling_parents = dbs.query(DbFolderItems.id).filter(DbFolderItems.folder_id != None).outerjoin(DbFolder, DbFolderItems.folder_id == DbFolder.id).filter(DbFolder.id == None).subquery()
        if cnt := dbs.query(DbFolderItems).filter(DbFolderItems.id.in_(sqlalchemy.select(dangling_parents))).delete(synchronize_session=False):
            log.error(f"Deleted {cnt} DbFolderItem rows, referencing parent folders that didn't exist in DbFolder. THIS IS A FOREIGN KEY VIOLATION!")

        dangling_subfolders = dbs.query(DbFolderItems.id).filter(DbFolderItems.subfolder_id != None).outerjoin(DbFolder, DbFolderItems.subfolder_id == DbFolder.id).filter(DbFolder.id == None).subquery()
        if cnt := dbs.query(DbFolderItems).filter(DbFolderItems.id.in_(sqlalchemy.select(dangling_subfolders))).delete(synchronize_session=False):
            log.error(f"Deleted {cnt} DbFolderItem rows, referencing subfolders that didn't exist in DbFolder. THIS IS A FOREIGN KEY VIOLATION!")

        dangling_media_files = dbs.query(DbFolderItems.id).filter(DbFolderItems.media_file_id != None).outerjoin(DbMediaFile, DbFolderItems.media_file_id == DbMediaFile.id).filter(DbMediaFile.id == None).subquery()
        if cnt := dbs.query(DbFolderItems).filter(DbFolderItems.id.in_(sqlalchemy.select(dangling_media_files))).delete(synchronize_session=False):
            log.error(f"Deleted {cnt} media item from DbFolderItems that didn't exist in DbMediaFile. THIS COULD BE A BUG.")


def db_check_for_folder_loops(dbs: Session, log: Logger) -> bool:
    """
    Check the folder structure for loops.
    They can cause root folders to be unreachable, and infinite loops in the UI.
    """
    log.debug("Checking for folder loops...")

    def find_cycles(edges: List[Tuple[int, int]]) -> List[List[int]]:
        graph: dict[int,list[int]] = {n: [] for e in edges for n in e}
        for u, v in edges: graph[u].append(v)
        visited, rec_stack, cycles = set(), set(), []

        def dfs(v: int, path: List[int]) -> None:
            if v in rec_stack:
                cycles.append(path[path.index(v):])
                return
            if v not in visited:
                visited.add(v)
                rec_stack.add(v)
                path.append(v)
                for n in graph[v]: dfs(n, path)
                path.pop()
                rec_stack.remove(v)

        for node in graph:
            if node not in visited: dfs(node, [])
        return cycles

    with dbs.begin():
        rows = dbs.query(DbFolderItems.folder_id, DbFolderItems.subfolder_id).filter(DbFolderItems.subfolder_id != None).all()
        if not rows:
            log.debug("(No folder structure found, skipping loop check.)")
            return False

        if cycles := find_cycles([(r.folder_id, r.subfolder_id) for r in rows]):
            for i, cyc in enumerate(cycles):
                cycle_description = " -> ".join(f"Folder (id: {v})" for v in cyc) + f" -> Folder (id: {cyc[0]})"
                log.error(f"!! Found a folder loop ({i+1} of {len(cycles)}): {cycle_description}")
                mentioned_folder_objs = dbs.query(DbFolder).filter(DbFolder.id.in_(set(cyc))).all()
                log.error(f" - Folders involved: {'; '.join(f'#{f.id} (`{f.title}`) user `{f.user_id}`' for f in mentioned_folder_objs)}")
                log.error(f" -> Breaking the loop now by removing the last edge (from {cyc[-1]} to {cyc[0]})")
                cnt = dbs.query(DbFolderItems).filter(DbFolderItems.folder_id == cyc[-1], DbFolderItems.subfolder_id == cyc[0]).delete()
                dbs.flush()
                assert cnt == 1, "Expected to delete exactly one row"
            return True
        else:
            log.debug("No loops detected in the folder structure.")
            return False


async def db_get_or_create_user_root_folder(dbs: Session, user: clap.UserInfo, srv: Optional[org.OrganizerOutboundStub], log: Logger) -> DbFolder:
    """
    Find the folder with no parent for the user.
    If none is found, create one and move all non-parent media files to it.
    """
    user_msg = None     # Queue any user message, as transaction will block it from being stored in the DB otherwise

    with dbs.begin_nested():
        assert user and user.id and user.name, "User ID and name must be set"

        # DEBUG: Check that the user exists (DbUser table)
        if not dbs.query(DbUser).filter(DbUser.id == user.id).one_or_none():
            raise ValueError(f"User '{user.id}' not found in DbUser table")

        # Find DbFolder(s) with no parent for the user
        res = dbs.query(DbFolder).filter(DbFolder.user_id == user.id).outerjoin(DbFolderItems, DbFolder.id == DbFolderItems.subfolder_id).filter(DbFolderItems.subfolder_id.is_(None)).all()
        cnt, ret = len(res), (res[0] if res else None)

        if cnt > 1:
            # Should not happen, otherwise DB is in an inconsistent state
            log.error(f"Multiple root folders found for user {user.id}! Moving newer ones into the first created one.")
            oldest_fld = min(res, key=lambda f: f.id)
            for f in res:
                if f.id != oldest_fld.id:
                    dbs.add(DbFolderItems(folder_id=oldest_fld.id, subfolder_id=f.id))
                    dbs.flush()
            ret = oldest_fld
            user_msg = clap.UserMessage(
                message="Multiple root folders in DB. Please report to support.",
                details="Users should have one root folder. This inconsistency was fixed, but your home view might contain unexpected folders.",
                type=clap.UserMessageType.ERROR)

        elif cnt == 0:
            # Create a root folder & move all orphan media files to it
            assert ret is None
            log.info(f"No root folder for user '{user.id}', creating one now.")
            ret = DbFolder(user_id=user.id, title=f"Home of '{user.name}'")
            dbs.add(ret)
            dbs.flush() # make sure the ret.id is set

        assert ret is not None, "Unexpected None result"

        # Move all orphan media files to the root folder (whether or not we created it just now)
        dbs.execute(sqla_text(dedent('''
            INSERT INTO bf_folder_items (folder_id, media_file_id)
            SELECT :root_folder_id, v.id FROM media_files v
            LEFT JOIN bf_folder_items bfi ON v.id = bfi.media_file_id AND bfi.media_file_id IS NOT NULL
            WHERE v.user_id = :user_id AND bfi.media_file_id IS NULL;
        ''')), {"user_id": user.id, "root_folder_id": ret.id})

    if user_msg and srv:
        if err := await try_send_user_message(srv, org.ClientShowUserMessageRequest(user_persist=user.id, msg=user_msg)):
            log.error(f"Error sending user message: {err}")
    elif user_msg and not srv:
        log.warning("No server connection to send user message: {user_msg}")

    return ret
