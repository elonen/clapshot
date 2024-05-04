from typing import Any, Optional, cast, Iterable
import uuid

from dataclasses import dataclass
from datetime import datetime
from textwrap import dedent
from logging import Logger

import sqlalchemy
from sqlalchemy import ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, DeclarativeBase, relationship, Session

import clapshot_grpc.clapshot.organizer as org
import clapshot_grpc.clapshot as clap

# Database ORM mappings

class Base(DeclarativeBase):
    pass


class DbFolder(Base):
    __tablename__ = "bf_folders"
    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    created: Mapped[datetime] = mapped_column(insert_default=sqlalchemy.func.now())
    user_id: Mapped[str] = mapped_column()
    title: Mapped[str] = mapped_column()

    # ORM relationships (objects, not keys)
    #items = relationship("DbFolderItems", primaryjoin="DbFolder.id==DbFolderItems.folder_id")
    #parent = relationship("DbFolder", secondary="bf_folder_items", primaryjoin="DbFolder.id==DbFolderItems.folder_id", secondaryjoin="DbFolder.id==DbFolderItems.subfolder_id", uselist=False, remote_side="DbFolder.id")
    #children = relationship("DbFolder", secondary="bf_folder_items", primaryjoin="DbFolder.id==DbFolderItems.subfolder_id", secondaryjoin="DbFolder.id==DbFolderItems.folder_id", remote_side="DbFolder.id")


class DbFolderItems(Base):
    __tablename__ = "bf_folder_items"
    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)

    folder_id: Mapped[Optional[int]] = mapped_column(ForeignKey("bf_folders.id", ondelete="CASCADE", onupdate="CASCADE"))
    sort_order: Mapped[int] = mapped_column(default=0)
    # "Enum" -- one of these two columns must be set
    video_id: Mapped[Optional[str]] = mapped_column(ForeignKey("videos.id", ondelete="CASCADE", onupdate="CASCADE"), unique=True, nullable=True)
    subfolder_id: Mapped[Optional[int]] = mapped_column(ForeignKey("bf_folders.id", ondelete="CASCADE", onupdate="CASCADE"), unique=True, nullable=True)

    # Constraints
    constraint_enum = sqlalchemy.CheckConstraint("(video_id IS NULL) != (subfolder_id IS NULL)", name="video_id_xor_subfolder_id")
    constraint_self_ref = sqlalchemy.CheckConstraint("folder_id != subfolder_id", name="folder_id_ne_subfolder_id")
    __table_args__ = (constraint_enum, constraint_self_ref)


class DbSchemaMigrations(Base):
    __tablename__ = "__bf_schema_migrations"
    version: Mapped[str] = mapped_column(primary_key=True)
    migration_uuid: Mapped[str] = mapped_column()
    run_on: Mapped[datetime] = mapped_column(insert_default=sqlalchemy.func.now())


# Not managed by the organizer migrations, but by the clapshot.server module.
class DbVideo(Base):
    __tablename__ = "videos"
    id: Mapped[str] = mapped_column(primary_key=True)
    user_id: Mapped[str] = mapped_column()
    user_name: Mapped[str] = mapped_column()
    added_time: Mapped[datetime] = mapped_column(insert_default=sqlalchemy.func.now())
    recompression_done: Mapped[Optional[datetime]] = mapped_column()
    orig_filename: Mapped[str] = mapped_column()
    total_frames: Mapped[int] = mapped_column()
    duration: Mapped[float] = mapped_column()
    fps: Mapped[str] = mapped_column()
    raw_metadata_all: Mapped[str] = mapped_column()
    title: Mapped[str] = mapped_column()
    thumb_sheet_cols: Mapped[int] = mapped_column()
    thumb_sheet_rows: Mapped[int] = mapped_column()

    # ORM relationships (objects, not keys)
    #folders = relationship("DbFolder", secondary="bf_folder_items", primaryjoin="DbVideo.id==DbFolderItems.video_id", secondaryjoin="DbFolder.id==DbFolderItems.folder_id")


# List of all specified migrations.
#
# Notes:
# - `uuid` is an arbitrary unique id for the migration
# - `version` is arbitrary, and _not_ unique, but must be sortable
#   - Multiple migrations with the same version make sense when they have different dependencies.
#     The engine will try to find a path to get all modules to their highest migration version,
#     by finding a path through the dependency graph.

@dataclass
class MigrationEntry:
    metadata: org.Migration
    up_sql: str

ALL_MIGRATIONS: list[MigrationEntry] = [

    MigrationEntry(
        metadata = org.Migration(
            uuid="basic_folders_2024-05-01_1610",
            version="0001_initial_schema",
            dependencies=[
                org.MigrationDependency(
                    name="clapshot.server",
                    min_ver="2023-04-18-190209_change_video_primkey",
                    max_ver=None
                ),
                org.MigrationDependency(
                    name="clapshot.organizer.basic_folders",
                    min_ver=None,
                    max_ver=""  # = database must have _no_ applied migrations yet for this module
                )
            ],
            description="Initial schema. One video per folder, folders are user-specific and can be nested."
        ),
        up_sql= dedent('''
                CREATE TABLE bf_folders (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
                    user_id VARCHAR(255) NOT NULL,
                    title VARCHAR(255) NOT NULL
                );

                CREATE INDEX bf_folders_user_id ON bf_folders(user_id);

                CREATE TABLE bf_folder_items (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,

                    folder_id INTEGER NOT NULL REFERENCES bf_folders(id) ON UPDATE CASCADE ON DELETE CASCADE,
                    sort_order INTEGER NOT NULL DEFAULT 0,

                    video_id VARCHAR(255) UNIQUE REFERENCES videos(id) ON UPDATE CASCADE ON DELETE CASCADE,
                    subfolder_id INTEGER UNIQUE REFERENCES bf_folders(id) ON UPDATE CASCADE ON DELETE CASCADE,

                    CHECK (
                        (video_id IS NOT NULL AND subfolder_id IS NULL) OR
                        (video_id IS NULL AND subfolder_id IS NOT NULL)
                    ),
                    CHECK (folder_id != subfolder_id)
                );

                CREATE INDEX bf_folder_items_folder_id ON bf_folder_items(folder_id);
                CREATE INDEX bf_folder_items_video_id ON bf_folder_items(video_id);
                CREATE INDEX bf_folder_items_subfolder_id ON bf_folder_items(subfolder_id);
                ''')
    ),

]


def db_check_pending_migrations(db: sqlalchemy.Engine) -> tuple[str, list[org.Migration]]:
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
                dbs.execute(sqlalchemy.text(sql))
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
        dbs.execute(sqlalchemy.text("INSERT INTO videos (id, user_id) VALUES (:id, :user_id)"), {"id": test_video, "user_id": test_user})
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
        dbs.execute(sqlalchemy.text(dedent('''
            INSERT INTO bf_folder_items (folder_id, video_id)
            SELECT :root_folder_id, v.id FROM videos v
            LEFT JOIN bf_folder_items bfi ON v.id = bfi.video_id AND bfi.video_id IS NOT NULL
            WHERE v.user_id = :user_id AND bfi.video_id IS NULL;
        ''')), {"user_id": ses.user.id, "root_folder_id": ret.id})

        return ret


# Note to self: find videos that are not in any folder:
'''
SELECT videos.* FROM videos
LEFT JOIN bf_video_folders ON videos.id = bf_video_folders.video_id
WHERE bf_video_folders.video_id IS NULL AND videos.user_id = '<user_id>';
'''