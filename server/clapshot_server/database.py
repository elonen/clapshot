import logging
from pathlib import Path

import sqlalchemy as sql
from sqlalchemy.ext.declarative import declarative_base
from sqlalchemy import Column
from sqlalchemy.orm import relationship, backref, sessionmaker, joinedload

from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.ext.asyncio import create_async_engine
from sqlalchemy.future import select

Base = declarative_base() # type: sql.ext.declarative.api.DeclarativeMeta

class Video(Base):
    __tablename__ = 'video'
    __mapper_args__ = {"eager_defaults": True}

    id = sql.Column(sql.Integer, primary_key=True)  # required by SQLAlchemy
    video_hash = Column(sql.String, index=True, unique=True)

    added_by_userid = Column(sql.String, index=True)    # unique user id
    added_by_username = Column(sql.String)  # human readable username
    added_time = Column(sql.DateTime, server_default=sql.func.now(), nullable=False)

    recompression_done = Column(sql.DateTime, nullable=True, default=None)
    orig_filename = Column(sql.String)

    total_frames = Column(sql.Integer)
    duration = Column(sql.Float)
    fps = Column(sql.String) # decimal number in seconds

    raw_metadata_video = Column(sql.String)
    raw_metadata_all = Column(sql.String)

    comments = relationship("Comment", cascade="all, delete-orphan")
    
    def to_dict(self):
        return {
            "video_hash": self.video_hash,
            "added_by_userid": self.added_by_userid,
            "added_by_username": self.added_by_username,
            "added_time": self.added_time.isoformat(),
            "orig_filename": self.orig_filename,
            "total_frames": self.total_frames,
            "duration": self.duration,
            "fps": self.fps,
            "raw_metadata_video": self.raw_metadata_video,
            "raw_metadata_all": self.raw_metadata_all
        }

    def __repr__(self):
       return f"<Video(id='{self.id}' video_hash='{self.video_hash}' orig_filename='{self.orig_filename}' added_by_userid='{self.added_by_userid}' ...)>"


class Comment(Base):
    __tablename__ = 'comment'
    __mapper_args__ = {"eager_defaults": True}

    id = Column(sql.Integer, primary_key=True)  # required by SQLAlchemy
    video_hash = Column(sql.Integer, sql.ForeignKey('video.video_hash'), nullable=False)

    parent_id = Column(sql.Integer, sql.ForeignKey('comment.id'), default=None, index=True)
    created = Column(sql.DateTime, server_default=sql.func.now(), nullable=False)
    edited = Column(sql.DateTime, default=None)  # set if comment has been edited since creation
    user_id = Column(sql.String, default="anonymous")    # unique user id
    username = Column(sql.String, default="Anonymous")   # human readable username
    comment = Column(sql.String, default="")
    drawing = Column(sql.String, default=None)  # image data URI

    def to_dict(self):
        return {
            "id": self.id,
            "video_hash": self.video_hash,
            "parent_id": self.parent_id,
            "created": self.created.isoformat(),
            "edited": self.edited.isoformat() if self.edited else None,
            "user_id": self.user_id,
            "username": self.username,
            "comment": self.comment,
            "drawing": self.drawing
        }

    def __repr__(self):
       return f"<Comment(id='{self.id}' video={self.video_hash} parent={self.parent_id} user_id='{self.user_id}' comment='{self.comment}' has-drawing={not(not self.drawing)} ...)>"





class Database:
    def __init__(self, db_file: Path, logger: logging.Logger):
        self.logger = logger
        self.db_file = db_file

    async def __aenter__(self):
        self.engine = create_async_engine(f"sqlite+aiosqlite:///{self.db_file}", echo=False)

        async with self.engine.begin() as c:
            await c.run_sync(Base.metadata.create_all, checkfirst=True)

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
            await session.execute(sql.update(Comment).filter_by(id=comment_id).values(comment=new_comment))
            await session.commit()
