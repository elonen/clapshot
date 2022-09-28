from email.policy import default
import logging
from pathlib import Path

import sqlalchemy as sql
from sqlalchemy.ext.declarative import declarative_base
from sqlalchemy import Column
from sqlalchemy.orm import relationship, backref, sessionmaker, joinedload

from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.ext.asyncio import create_async_engine
from sqlalchemy.future import select

from alembic.config import Config as AlembicCfg
from alembic import command as alembic_cmd
from alembic import script as alembic_script
from alembic.runtime import migration

Base = declarative_base() # type: sql.ext.declarative.api.DeclarativeMeta

class Video(Base):
    __tablename__ = 'video'
    __mapper_args__ = {"eager_defaults": True}

    id = sql.Column(sql.Integer, primary_key=True, autoincrement=True)  # required by SQLAlchemy
    video_hash = Column(sql.String, index=True, unique=True)

    added_by_userid = Column(sql.String, index=True)    # unique user id
    added_by_username = Column(sql.String)  # human readable username
    added_time = Column(sql.DateTime, server_default=sql.func.now(), nullable=False)

    recompression_done = Column(sql.DateTime, nullable=True, default=None)
    orig_filename = Column(sql.String)

    total_frames = Column(sql.Integer)
    duration = Column(sql.Float)
    fps = Column(sql.String) # decimal number in seconds

    raw_metadata_all = Column(sql.String)

    comments = relationship("Comment", cascade="all, delete-orphan")
    
    def to_dict(self):
        return {
            "video_hash": self.video_hash,
            "added_by_userid": self.added_by_userid,
            "added_by_username": self.added_by_username,
            "added_time": self.added_time.isoformat() if self.added_time else None,
            "orig_filename": self.orig_filename,
            "total_frames": self.total_frames,
            "duration": self.duration,
            "fps": self.fps,
            "raw_metadata_all": self.raw_metadata_all
        }

    def __repr__(self):
       return f"<Video(id='{self.id}' video_hash='{self.video_hash}' orig_filename='{self.orig_filename}' added_by_userid='{self.added_by_userid}' ...)>"


class Comment(Base):
    __tablename__ = 'comment'
    __mapper_args__ = {"eager_defaults": True}
    __table_args__ = {'sqlite_autoincrement': True} # required to avoid ID reuse

    id = Column(sql.Integer, primary_key=True, autoincrement=True)
    video_hash = Column(sql.Integer, sql.ForeignKey('video.video_hash'), nullable=False)

    parent_id = Column(sql.Integer, sql.ForeignKey('comment.id'), default=None, index=True)
    created = Column(sql.DateTime, server_default=sql.func.now(), nullable=False)
    edited = Column(sql.DateTime, default=None)  # set if comment has been edited since creation
    user_id = Column(sql.String, default="anonymous")    # unique user id
    username = Column(sql.String, default="Anonymous")   # human readable username
    comment = Column(sql.String, default="")
    timecode = Column(sql.String, default=None, nullable=True)
    drawing = Column(sql.String, default=None, nullable=True)  # image data URI

    def to_dict(self):
        return {
            "comment_id": self.id,
            "video_hash": self.video_hash,
            "parent_id": self.parent_id if self.parent_id else '',
            "created": self.created.isoformat() if self.created else None,
            "edited": self.edited.isoformat() if self.edited else None,
            "user_id": self.user_id,
            "username": self.username,
            "comment": self.comment,
            "timecode": self.timecode or '',
            "drawing": self.drawing
        }

    def __repr__(self):
       return f"<Comment({self.id} video={self.video_hash} parent={self.parent_id or '-'} user_id='{self.user_id}' comment='{self.comment}' drawing={not(not self.drawing)} ...)>"


class Message(Base):
    __tablename__ = 'message'
    __mapper_args__ = {"eager_defaults": True}
    __table_args__ = {'sqlite_autoincrement': True} # required to avoid ID reuse

    id = Column(sql.Integer, primary_key=True, autoincrement=True)
    user_id = Column(sql.String, default="anonymous")
    created = Column(sql.DateTime, server_default=sql.func.now(), nullable=False)
    seen = Column(sql.Boolean, default=False)

    ref_video_hash = Column(sql.Integer, sql.ForeignKey('video.video_hash'), nullable=True, default=None)
    ref_comment_id = Column(sql.Integer, sql.ForeignKey('comment.id'), nullable=True, default=None)

    event_name = Column(sql.String, default="info")  # info, warning, error
    message = Column(sql.String, default="")
    details = Column(sql.String, default="")


    def to_dict(self):
        return {
            "id": self.id,
            "user_id": self.user_id,
            "created": self.created.isoformat() if self.created else None,
            "seen": self.seen,
            "ref_video_hash": self.ref_video_hash,
            "ref_comment_id": self.ref_comment_id,
            "event_name": self.event_name,
            "message": self.message,
            "details": self.details
        }

    def __repr__(self):
         return f"<Message({self.id} user_id='{self.user_id}' video={self.ref_video_hash} comment={self.ref_comment_id} seen={self.seen} event_name='{self.event_name}' message='{self.message}' ...)>"



class Database:
    def __init__(self, db_file: Path, logger: logging.Logger):
        self.logger = logger
        self.db_file = db_file
        self.error_state = "db not started (__aenter__ not called)"

    async def __aenter__(self):
        db_existed = self.db_file.exists()
        self.engine = create_async_engine(f"sqlite+aiosqlite:///{self.db_file}", echo=False)

        alb_cfg = AlembicCfg(Path(__file__).parent / 'alembic.ini')
        alb_cfg.set_main_option("script_location", "clapshot_server:sqlite-migrations")
        alb_dir = alembic_script.ScriptDirectory.from_config(alb_cfg)

        if not db_existed:
            # Create database
            async with self.engine.begin() as c:
                self.logger.info("Creating tables for " + str(self.db_file))
                def doit(sync_c):
                    Base.metadata.create_all(sync_c)
                    # Stamp the newly created database with latest migration
                    migration.MigrationContext.configure(sync_c).stamp(alb_dir, "head")
                    self.error_state = None
                await c.run_sync(doit)
        else:
            # Make sure database is at latest migration
            async with self.engine.begin() as c:
                def is_latest_migration(sync_c):
                    ctx = migration.MigrationContext.configure(sync_c)
                    return set(ctx.get_current_heads()) == set(alb_dir.get_heads())

                self.error_state = None if await c.run_sync(is_latest_migration) else \
                    "Database schema is out of sync with app. Use 'clapshot-alembic' to upgrade it."

        if not self.error_state:
            self.async_session = sessionmaker(self.engine, expire_on_commit=False, class_=AsyncSession)
        
        return self

    async def __aexit__(self, exc_t, exc_v, exc_tb):
        await self.engine.dispose()


    # Video
    # -----
    async def add_video(self, video: Video) -> sql.Integer:
        async with self.async_session() as session:
            session.add(video)
            await session.commit()
            return video.id

    async def set_video_recompressed(self, video_hash: str):
        async with self.async_session() as session:
            await session.execute(sql.update(Video).filter_by(video_hash=video_hash).values(recompression_done=sql.func.now()))
            await session.commit()

    async def get_video(self, video_hash: str) -> Video:
        async with self.async_session() as session:
            res = await session.execute(select(Video).filter_by(video_hash=video_hash))
            return res.scalars().first()

    async def del_video_and_comments(self, video_hash: str):
        async with self.async_session() as session:
            await session.execute(sql.delete(Video).filter_by(video_hash=video_hash))
            await session.execute(sql.delete(Comment).filter_by(video_hash=video_hash))
            await session.commit()

    async def get_all_user_videos(self, user_id: str) -> list[Video]:
        async with self.async_session() as session:
            res = await session.execute(select(Video).filter_by(added_by_userid=user_id))
            return res.scalars().all()

    # Comment
    # -------
    async def add_comment(self, comment: Comment) -> sql.Integer:
        async with self.async_session() as session:
            session.add(comment)
            await session.commit()
            assert comment.parent_id is None or comment.parent_id != comment.id, "Comment cannot be its own parent"
            return comment.id

    async def get_comment(self, comment_id: int) -> Comment:
        async with self.async_session() as session:
            return (await session.execute(select(Comment).filter_by(id=comment_id))).scalars().first()

    async def get_video_comments(self, video_hash: str) -> list[Comment]:
        async with self.async_session() as session:
            res = (await session.execute(select(Comment).filter_by(video_hash=video_hash))).scalars().all()
            return res
    
    async def del_comment(self, comment_id: int) -> None:
        async with self.async_session() as session:
            await session.execute(sql.delete(Comment).filter_by(id=comment_id))
            await session.commit()

    async def edit_comment(self, comment_id: int, new_comment: str) -> None:
        async with self.async_session() as session:
            await session.execute(sql.update(Comment).filter_by(id=comment_id).values(comment=new_comment, edited=sql.func.now()))
            await session.commit()

    # Message
    # -------
    async def add_message(self, msg: Message) -> Message:
        async with self.async_session() as session:
            session.add(msg)
            await session.commit()
            return msg   # Contains new id and timestamp
    
    async def get_message(self, msg_id: int) -> Message:
        async with self.async_session() as session:
            return (await session.execute(select(Message).filter_by(id=msg_id))).scalars().first()
    
    async def get_user_messages(self, user_id: str) -> list[Message]:
        async with self.async_session() as session:
            res = (await session.execute(select(Message).filter_by(user_id=user_id))).scalars().all()
            return res

    async def set_message_seen(self, msg_id: int, new_status: bool) -> None:
        async with self.async_session() as session:
            await session.execute(sql.update(Message).filter_by(id=msg_id).values(seen=new_status))
            await session.commit()
    
    async def del_message(self, msg_id: int) -> None:
        async with self.async_session() as session:
            await session.execute(sql.delete(Message).filter_by(id=msg_id))
            await session.commit()
