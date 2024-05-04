from typing import Optional
from datetime import datetime

import sqlalchemy
from sqlalchemy import ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, DeclarativeBase, relationship


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
