import asyncio
from contextlib import suppress
from dataclasses import dataclass
import logging
from typing import Any, DefaultDict, Optional
from aiohttp import web
from pathlib import Path
import socketio

from .database import Database, Video, Comment

sio = socketio.AsyncServer(async_mode='aiohttp')
SOCKET_IO_PATH = '/api/socket.io'

@dataclass
class CustomUserMessage:
    """
    Custom message to be sent to the client.
    """
    user_id: str 
    fields: dict[str, Any]
    event_name: str = 'info'


class ClapshotApiServer:

    def __init__(self, db: Database, url_base: str, port: int, logger: logging.Logger):
        self.db = db
        self.url_base = url_base
        self.port = port
        self.logger = logger

        self.app = web.Application()
        sio.attach(self.app, socketio_path=SOCKET_IO_PATH)

        self.userid_to_sid: dict[str, str] = {}


        @sio.event
        async def list_my_videos(sid, msg):
            try:
                user_id, username = await self.get_user(sid)
                self.logger.info(f"list_my_videos: user_id='{user_id}' username='{username}'")
                videos = await self.db.get_all_user_videos(user_id)
                await sio.emit('user_videos', {
                    'username': username,
                    'user_id': user_id,
                    'videos': [self.dict_for_video(v) for v in videos] 
                    }, room=sid)
            except Exception as e:
                self.logger.exception(f"Exception in list_my_videos for sid '{sid}':: {e}")
                await sio.emit('error', {'msg': f"Failed to list videos for you:: {e}"}, room=sid)


        @sio.event
        async def open_video(sid, msg):
            try:
                user_id, username = await self.get_user(sid)
                video_hash = msg['video_hash']
                self.logger.info(f"lookup_video: user_id='{user_id}' username='{username}' video_hash='{video_hash}'")                
                v = await self.db.get_video(video_hash)
                if v is None:
                    await sio.emit('error', {'msg': f"No such '{video_hash}'"}, room=sid)
                else:
                    sio.enter_room(sid, video_hash)
                    await sio.emit('open_video', self.dict_for_video(v), room=sid)
            except Exception as e:
                self.logger.exception(f"Exception in lookup_video for sid '{sid}':: {e}")
                await sio.emit('error', {'msg': f"Failed to lookup video:: {e}"}, room=sid)

        @sio.event
        async def del_video(sid, msg):
            try:
                user_id, username = await self.get_user(sid)
                video_hash = msg['video_hash']
                self.logger.info(f"del_video: user_id='{user_id}' video_hash='{video_hash}'")
                v = await self.db.get_video(video_hash)
                if v is None:
                    await sio.emit('error', {'msg': f"No such video '{video_hash}'. Cannot delete."}, room=sid)
                else:
                    assert user_id in (v.added_by_userid, 'admin'), f"Video '{video_hash}' not owned by you. Cannot delete."
                    await self.db.del_video_and_comments(video_hash)
                    await sio.emit('info', {'msg': f"Deleted video '{video_hash}' / '{v.orig_filename}'"}, room=sid)

            except Exception as e:
                self.logger.exception(f"Exception in del_video for sid '{sid}':: {e}")
                await sio.emit('error', {'msg': f"Failed to delete video:: {e}"}, room=sid)



        @sio.event
        async def add_comment(sid, msg):
            try:
                user_id, username = await self.get_user(sid)
                assert user_id and username
                video_hash = msg['video_hash']

                vid = await self.db.get_video(video_hash)
                if vid is None:
                    await sio.emit('error', {'msg': f"No such video '{video_hash}'. Cannot comment."}, room=sid)
                    return

                comment = Comment(
                    video_hash = video_hash,
                    parent_id = msg.get('parent_id'),
                    user_id = user_id,
                    username = username,
                    comment = str(msg.get('comment')) or '',
                    drawing = str(msg.get('drawing')) or None)

                await self.db.add_comment(comment)
                await sio.emit('new_comment', comment.to_dict(), room=video_hash)

            except Exception as e:
                self.logger.exception(f"Exception in add_comment for sid '{sid}':: {e}")
                await sio.emit('error', {'msg': f"Failed to add comment:: {e}"}, room=sid)


        @sio.event
        async def edit_comment(sid, msg):
            try:
                user_id, username = await self.get_user(sid)
                assert user_id and username
                comment_id = msg['comment_id']
                comment = str(msg['comment'])
                
                old = await db.get_comment(comment_id)
                video_hash = old.video_hash
                assert user_id in (old.user_id, 'admin'), "You can only edit your own comments"

                await self.db.edit_comment(comment_id, comment)
                await sio.emit('del_comment', {'comment_id': comment_id}, room=video_hash)
                await sio.emit('new_comment', {'comment_id': comment_id, 'comment': comment}, room=video_hash)

            except Exception as e:
                self.logger.exception(f"Exception in edit_comment for sid '{sid}':: {e}")
                await sio.emit('error', {'msg': f"Failed to edit comment:: {e}"}, room=sid)


        @sio.event
        async def del_comment(sid, msg):
            try:
                user_id, username = await self.get_user(sid)
                assert user_id and username
                comment_id = msg['comment_id']

                old = await self.db.get_comment(comment_id)
                video_hash = old.video_hash
                assert user_id in (old.user_id, 'admin'), "You can only delete your own comments"

                all_comm = await self.db.get_video_comments(video_hash)
                for c in all_comm:
                    if c.parent_id == comment_id:
                        raise Exception("Can't delete a comment that has replies")
            
                await self.db.del_comment(comment_id)
                await sio.emit('del_comment', {'comment_id': comment_id}, room=video_hash)

            except Exception as e:
                self.logger.exception(f"Exception in del_comment for sid '{sid}':: {e}")
                await sio.emit('error', {'msg': f"Failed to delete comment:: {e}"}, room=sid)

        @sio.event
        async def logout(sid):
            await sio.disconnect(sid)

        @sio.event
        async def connect(sid, environ):
            # Trust headers from web server / reverse proxy on user auth
            user_id, username = self.user_from_headers(environ)
            
            await sio.save_session(sid, {'user_id': user_id, 'username': username})
            self.userid_to_sid[user_id] = sid            
            await sio.emit('my_response', {'data': 'Connected', 'count': 0, 'user_id': user_id})
            sio.enter_room(sid, 'huoneusto')

        @sio.event
        async def disconnect(sid):
            user_id = (await sio.get_session(sid)).get('user_id')
            print(f'Client disconnected, sid={sid}, user_id={user_id}')
            self.userid_to_sid.pop(user_id, None)

        # HTTP routes
        async def index(request: web.Request):
            module_dir = Path(__file__).parent.absolute()
            with open(module_dir/'static/index.html') as f:
                return web.Response(text=f.read(), content_type='text/html')

        self.app.router.add_static('/api/static', Path(__file__).parent.absolute()/'static')
        self.app.router.add_get('/api', index)


    async def push_message(self, msg: CustomUserMessage):
        assert msg.user_id
        if msg.user_id in self.userid_to_sid:
            await sio.emit(msg.event_name, msg.fields, room=self.userid_to_sid[msg.user_id])


    def dict_for_video(self, v: Video) -> dict:
        return {
                'orig_filename': v.orig_filename,
                'video_hash': v.video_hash,
                'video_url': self.url_base.rstrip('/') + ('' if self.port in (80, 443) else f':{self.port}') + f'/video/{v.video_hash}/video.mp4',
                'added_time': str(v.added_time.isoformat()),
                'duration': v.duration,
                'username': v.added_by_username,
                'user_id': v.added_by_userid
                }


    def user_from_headers(self, headers: Any) -> tuple[str, str]:
        """
        Get user id and username from (reverse proxy's) headers.

        return: (user_id, username)
        """
        user_id = headers.get('HTTP_X_REMOTE_USER_ID')
        if not user_id:
            self.logger.info("No user id found in header HTTP_X_REMOTE_USER_ID, using 'anonymous'")
        user_name = headers.get('HTTP_X_REMOTE_USER_NAME')
        return (user_id or 'anonymous', user_name or 'Anonymous')

    async def get_user(self, sid: str) -> tuple[str, str]:
        session = await sio.get_session(sid)
        return session.get('user_id'), session.get('username')



async def run_server(
        db: Database,
        logger: logging.Logger,
        url_base: str,
        push_messages: asyncio.Queue,
        host='localhost',
        port: int=8086
    ) -> None:
    """
    Run HTTP / Socket.IO API server forever (until this asyncio task is cancelled)

    Params:
        db:           Database object
        logger:       Logger instance for API server
        url_base:     Base URL for the server (e.g. https://example.com). Used e.g. to construct video URLs.
        port:         Port to listen on
    """
    async with db:
        server = ClapshotApiServer(db=db, url_base=url_base, port=port, logger=logger)
        runner = web.AppRunner(server.app)
        await runner.setup()
        # bind to localhost only, no matter what url_base is (for security, use reverse proxy to expose)
        site = web.TCPSite(runner, host, server.port)
        logger.info(f"Starting API server. Bound to {host}:{server.port} -- Base URL: {url_base}")
        await site.start()

        while True:
            # Wait for messages from other parts of the app
            msg = await push_messages.get()
            await server.push_message(msg)
