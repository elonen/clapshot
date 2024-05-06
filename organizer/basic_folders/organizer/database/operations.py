from typing import Optional
import uuid

from textwrap import dedent
from logging import Logger

import networkx
import sqlalchemy
from sqlalchemy import Engine, text as sqla_text
from sqlalchemy.orm import Session

import clapshot_grpc.clapshot.organizer as org
import clapshot_grpc.clapshot as clap

from .models import Base, DbFolder, DbFolderItems, DbSchemaMigrations, DbVideo
from .migrations import ALL_MIGRATIONS

def db_check_pending_migrations(db: Engine) -> tuple[str, list[org.Migration]]:
    """
    Check the database for current schema version and return a list of migrations that have a higher version string.
    """
    with db.connect() as conn:
        # Create the schema migrations table if it doesn't exist
        Base.metadata.create_all(db, tables=[Base.metadata.tables[ DbSchemaMigrations.__tablename__]], checkfirst=True)
        # List migrations whose version string is higher than the current version
        with Session(conn) as session:
            cur_ver: str = session.query(DbSchemaMigrations.version).order_by(DbSchemaMigrations.version.desc()).limit(1).scalar() or ''
            pending = [m.metadata for m in ALL_MIGRATIONS if m.metadata.version > cur_ver]
            return cur_ver, pending


def db_apply_migration(dbs: Session, migr_uuid: str):
    try:
        migration = next(m for m in ALL_MIGRATIONS if m.metadata.uuid == migr_uuid)
        assert migration.metadata.version, "Migration version must be set"
        with dbs.begin_nested():
            for sql in migration.up_sql.split(";"):
                dbs.execute(sqla_text(sql))
            dbs.add(DbSchemaMigrations(version=migration.metadata.version, migration_uuid=migration.metadata.uuid))
    except StopIteration:
        raise ValueError(f"Migration with id '{migr_uuid}' not found")


def db_test_orm_mappings(dbs: Session, log: Logger):
    """
    After migrations are applied, test the ORM mappings by inserting some data and checking that it can be read back.
    """
    log.debug("Testing ORM mappings...")
    with dbs.begin() as tx:
        rnd = uuid.uuid4().hex  # random string to avoid collisions
        test_video, test_user = f"video_{rnd}", f"user_{rnd}"

        # Insert a video by SQL, since we don't have a Video ORM class
        dbs.execute(sqla_text("INSERT INTO videos (id, user_id) VALUES (:id, :user_id)"), {"id": test_video, "user_id": test_user})
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
        video_entry = DbFolderItems(video_id=test_video, folder_id=child_folder.id)
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

        dangling_videos = dbs.query(DbFolderItems.id).filter(DbFolderItems.video_id != None).outerjoin(DbVideo, DbFolderItems.video_id == DbVideo.id).filter(DbVideo.id == None).subquery()
        if cnt := dbs.query(DbFolderItems).filter(DbFolderItems.id.in_(sqlalchemy.select(dangling_videos))).delete(synchronize_session=False):
            log.error(f"Deleted {cnt} video item from DbFolderItems that didn't exist in DbVideo. THIS COULD BE A BUG.")


def db_check_for_folder_loops(dbs: Session, log: Logger) -> bool:
    """
    Check the folder structure for loops.
    They shouldn't cause any crashes, but could confuse the user.
    """
    log.debug("Checking for folder loops...")

    graph: networkx.DiGraph = networkx.DiGraph()
    rows = dbs.query(DbFolderItems.folder_id, DbFolderItems.subfolder_id).filter(DbFolderItems.subfolder_id != None).all()
    for folder_id, subfolder_id in rows:
        graph.add_edge(folder_id, subfolder_id)

    try:
        cycle = networkx.find_cycle(graph, orientation='original')

        cycle_description = " -> ".join(f"Folder (id: {u})" for u,v,_dir in cycle) + f" -> Folder (id: {cycle[0][0]})"
        log.warning(f"! Found a folder loop: {cycle_description}")

        mentioned_folder_ids = set(u for u,v,_dir in cycle)
        mentioned_folder_objs = dbs.query(DbFolder).filter(DbFolder.id.in_(mentioned_folder_ids)).all()
        log.warning(f" - Folders involved: {'; '.join(f'#{f.id} ({f.title}) user {f.user_id}' for f in mentioned_folder_objs)}")

        return True
    except networkx.NetworkXNoCycle:
        log.debug("No loops detected in the folder structure.")
        return False


async def db_get_or_create_user_root_folder(dbs: Session, ses: org.UserSessionData, srv: Optional[org.OrganizerOutboundStub], log: Logger) -> DbFolder:
    """
    Find the folder with no parent for the user.
    If none is found, create one and move all non-parent videos to it.
    """
    with dbs.begin_nested():
        res = dbs.query(DbFolder).filter(DbFolder.user_id == ses.user.id).outerjoin(DbFolderItems, DbFolder.id == DbFolderItems.subfolder_id).filter(DbFolderItems.subfolder_id == None)
        cnt, ret = res.count(), res.first()

        if cnt > 1:
            # Should not happen, otherwise DB is in an inconsistent state
            log.error(f"Multiple root folders found for user {ses.user.id}. Please fix database manually.")
            if srv:
                await srv.client_show_user_message(org.ClientShowUserMessageRequest(
                    user_persist=ses.user.id,
                    msg = clap.UserMessage(
                        message="Multiple root folders in DB. Contact support.",
                        details="Each user should have exactly one root folder. This DB issue may hide some of your videos until fixed.",
                        type=clap.UserMessageType.ERROR)))
        elif cnt == 0:
            # Create a root folder & move all orphan videos to it
            assert ret is None
            log.info(f"No root folder for user '{ses.user.id}', creating one now.")
            ret = DbFolder(user_id=ses.user.id, title=f"Home for {ses.user.name}")
            dbs.add(ret)
            dbs.flush() # make sure the ret.id is set

        assert ret is not None, "Unexpected None result"

        # Move all orphan videos to the root folder (whether or not we created it just now)
        dbs.execute(sqla_text(dedent('''
            INSERT INTO bf_folder_items (folder_id, video_id)
            SELECT :root_folder_id, v.id FROM videos v
            LEFT JOIN bf_folder_items bfi ON v.id = bfi.video_id AND bfi.video_id IS NOT NULL
            WHERE v.user_id = :user_id AND bfi.video_id IS NULL;
        ''')), {"user_id": ses.user.id, "root_folder_id": ret.id})

        return ret
