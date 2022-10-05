"""
Websocket based API server for Clapshot.

Listens to connections from web UI, runs commands from users,
communicates with database and video ingestion pipeline.
Pushes video processing results to clients.

**Relies on reverse proxy for authentication**! Specifically,
it belives anything that it gets from the
``HTTP_X_REMOTE_USER_ID`` and ``HTTP_X_REMOTE_USER_NAME``
headers. Proxy is expected to authenticate users in some
manner (e.g. Kerberos agains AD) and set these headers.

If specified, also serves video files, but this is mainly for testing.
Nginx or Apache should be used in production for that.
"""

import asyncio
from collections import defaultdict
from decimal import Decimal
from email.policy import default
import hashlib
import logging
from typing import Any, DefaultDict, Union, Callable
from aiohttp import web
import aiohttp_cors
from uuid import uuid4
from pathlib import Path
import urllib.parse
import aiofiles
import shutil
from datauri import DataURI

from .database import Database, Video, Comment, Message


class ClapshotApiServer:
    """
    Websocket server to communicate with web client
    """

    def __init__(
            self, db: Database, 
            url_base: str,
            port: int,
            logger: logging.Logger,
            videos_dir: Path,
            upload_dir: Path,
            serve_dirs: dict[str, Path],
            ingest_callback: Callable[str, str]):
        self.db = db
        self.url_base = url_base
        self.port = port
        self.logger = logger
        self.videos_dir = videos_dir
        self.upload_dir = upload_dir
        self.serve_dirs = serve_dirs
        self.ingest_callback = ingest_callback
        self.session_counter = 0

        self.app = web.Application()

        msg_to_handler = {
            'list_my_videos': self.list_my_videos,
            'open_video': self.open_video,
            'del_video': self.del_video,
            'add_comment': self.add_comment,
            'edit_comment': self.edit_comment,
            'del_comment': self.del_comment,
            'list_my_messages': self.list_my_messages,
            'logout': self.logout,
        }

        # User id -> list of websocket sessions they have open
        self.userid_to_ws: DefaultDict[str, list[web.WebSocketResponse]] = defaultdict(list)

        # Video hash -> list of websocket connections that are watching it
        self.video_hash_to_ws: DefaultDict[str, list[web.WebSocketResponse]] = defaultdict(list)


        async def websocket_connection_handler(request):
            """
            Handle websocket connection. Read authentication headers
            and waif for messages.
            """
            ws = web.WebSocketResponse(autoclose=False, autoping=True, heartbeat=5)
            await ws.prepare(request)

            self.session_counter += 1
            ws.sid = f"SES#{self.session_counter}"

            try:
                user_id, username = self.user_from_headers(request.headers)
                self.logger.info(f"[{ws.sid}] New WS session. User '{user_id}' ({username})")
                self.userid_to_ws[user_id].append(ws)

                await self.emit_cmd('welcome', {'user_id': user_id, 'username': username}, ws)

                async for msg in ws:
                    if msg.type == web.WSMsgType.TEXT:
                        try:
                            json_msg = msg.json()
                            assert json_msg.get('cmd'), "No 'cmd' specified"
                            if json_msg.get('cmd') in msg_to_handler:
                                if data := json_msg.get('data'):
                                    assert isinstance(data, dict), "'data' must be a dict"
                                await msg_to_handler[json_msg['cmd']](data or {}, ws, user_id, username)
                            else:
                                self.logger.warning(f"[{ws.sid}] Unknown command {json_msg.get('cmd')}")
                                ws.send_json({'error': f'Unknown command "{json_msg.get("cmd")}"'})

                        except Exception as e:
                            self.logger.error(f"[{ws.sid}] Exception while handling message: {e}")
                            ws.send_json({'error': 'Internal server error handling your message. Please report this.'})

                    elif msg.type == web.WSMsgType.ERROR:
                        self.logger.info(f'[{ws.sid}] WS connection closed with exception {ws.exception()}')
                        break

            except Exception as e:
                self.logger.error(f"[{ws.sid}] Exception while handling websocket connection: {e}")

            finally:
                self.logger.info(f"[{ws.sid}] WS connection closed for '{user_id}' ({username})")

                self.userid_to_ws[user_id].remove(ws)

                # remove from video_hash_to_ws
                for video_hash, ws_list in list(self.video_hash_to_ws.items()):
                    if ws in ws_list:
                        ws_list.remove(ws)
                        if not ws_list:
                            del self.video_hash_to_ws[video_hash]

                try:
                    await ws.close()
                except Exception as e:
                    self.logger.error(f"[{ws.sid}] Exception while closing WS connection. Ignoring: {e}")
                                    
                return ws



        async def post_upload_file(request):
            """
            Receive a HTTP file upload from the client.
            """
            user_id, _ = self.user_from_headers(request.headers)
            assert user_id , "No user_id for upload"            
            self.logger.info(f"post_upload_file: user_id='{user_id}'")
            
            async for field in (await request.multipart()):
                if field.name != 'fileupload':
                    logger.debug(f"post_upload_file(): Skipping UNKNOWN Multipart POST field '{field.name}'")
                else:
                    filename = field.filename
                    assert str(Path(filename).name) == filename, "Filename must not contain path"
                    dst = Path(self.upload_dir) / str(uuid4()) / Path(filename).name
                    assert not dst.exists(), f"Upload dst '{dst}' already exists, even tough it was prefixed with uuid4. Bug??"

                    try:
                        logger.info(f"post_upload_file(): Saving uploaded file '{filename}' to '{dst}'")
                        dst.parent.mkdir(parents=True, exist_ok=True)

                        async with aiofiles.open(dst, 'wb') as outf:
                            inq = asyncio.Queue(8)
                            async def reader():
                                while c := await field.read_chunk():
                                    await inq.put(c)
                                await inq.put(None)
                            async def writer():
                                while c := await inq.get():
                                    await outf.write(c)
                            tasks = [reader(), writer()]
                            try:
                                await asyncio.gather(*tasks)
                            except Exception as e:
                                for t in tasks:
                                    t.cancel()
                                logger.exception(f"post_upload_file(): Exception while saving uploaded file '{filename}' to '{dst}'")
                                raise e

                    except PermissionError as e:
                        self.logger.error(f"post_upload_file(): Permission error saving '{filename}' to '{dst}': {e}")
                        return web.Response(status=500, text="Permission error saving upload")

                    self.logger.info(f"post_upload_file(): File saved to '{dst}'. Queueing for processing.")
                    if self.ingest_callback:
                        self.ingest_callback(dst, user_id)

                    return web.Response(status=200, text="Upload OK")

                return web.Response(status=400, text="No fileupload field in POST")


        # Register HTTP routes
        self.app.router.add_post('/api/upload', post_upload_file)
        self.app.add_routes([web.get("/api/ws", websocket_connection_handler)])
        for route, path in self.serve_dirs.items():
            self.app.router.add_static(route, path)

        # Configure CORS on all routes
        cors = aiohttp_cors.setup(self.app, defaults={
        "*": aiohttp_cors.ResourceOptions(
                allow_credentials=False,
                expose_headers="*",
                allow_headers="*"
            )})
        for route in list(self.app.router.routes()):
            cors.add(route)


    async def emit_cmd(self, 
            cmd: str, 
            data: dict[str, Any],
            send_to: Union[str, web.WebSocketResponse]):
        """
        Send a message to a websocket connection.

        If send_to is a string, it is interpreted either as a video hash or user id.
        - If it turns out to be a video hash, the message is sent to all websocket
          that are watching it.
        - If it's a user id, the message is sent to all websocket connections that user has open.
        - If it's a WebSocketResponse, the message is sent to that connection only.
        """
        if isinstance(send_to, str):
            for ws in self.video_hash_to_ws.get(send_to, []):
                await ws.send_json({'cmd': cmd, 'data': data})
            for ws in self.userid_to_ws.get(send_to, []):
                await ws.send_json({'cmd': cmd, 'data': data})
        else:
            await send_to.send_json({'cmd': cmd, 'data': data})


    async def list_my_videos(self, data: dict, ws: web.WebSocketResponse, user_id: str, username: str):
        try:
            self.logger.info(f"[{ws.sid}] list_my_videos: user_id='{user_id}' username='{username}'")
            videos = await self.db.get_all_user_videos(user_id)
            await self.emit_cmd('user_videos', {
                'username': username,
                'user_id': user_id,
                'videos': [self.dict_for_video(v) for v in videos] 
                }, send_to=ws)
        except Exception as e:
            self.logger.exception(f"[{ws.sid}] Exception in list_my_videos for '{user_id}': {e}")
            await self.push_message(dont_throw=True, msg=Message(
                event_name='error', user_id=user_id,
                message=f"Failed to list your videos.", details=str(e)))


    async def _emit_new_comment(self, c: Comment, send_to: Union[str, web.WebSocketResponse]):
        """
        Helper function to send comment. Reads drawing
        from file if present and encodes into data URI.

        :param c: Comment to send
        :param room: Video hash or user id to send to
        """
        data = c.to_dict()
        if c.drawing and not c.drawing.startswith('data:'):
            if str(c.drawing).startswith('data:'):
                self.logger.warning(f"Comment '{c.id}' has data URI drawing stored in DB. Should be on disk.")
            path = self.videos_dir / str(c.video_hash) / 'drawings' / c.drawing
            if path.exists():
                async with aiofiles.open(path, 'rb') as f:
                    data['drawing'] = DataURI.make('image/webp', charset='utf-8', base64=True, data=await f.read())
            else:
                data['comment'] += ' [DRAWING NOT FOUND]'
        await self.emit_cmd('new_comment', data, send_to=send_to)


    async def open_video(self, data: dict, ws: web.WebSocketResponse, user_id: str, username: str):
        try:
            video_hash = str(data['video_hash'])
            self.logger.info(f"[{ws.sid}] open_video: user_id='{user_id}' username='{username}' video_hash='{video_hash}'")

            v = await self.db.get_video(video_hash)
            if v is None:
                await self.push_message(Message(
                    event_name='error', user_id=user_id,
                    ref_video_hash=video_hash,
                    message=f"No such video."))
            else:
                self.video_hash_to_ws[video_hash].append(ws)
                fields = self.dict_for_video(v)
                await self.emit_cmd('open_video', fields, send_to=ws)
                for c in await self.db.get_video_comments(video_hash):
                    self.logger.debug(f"[{ws.sid}] Sending to user id='{user_id}' comment {c}")
                    await self._emit_new_comment(c, send_to=ws)

        except Exception as e:
            self.logger.exception(f"[{ws.sid}] Exception in lookup_video for user '{user_id}': {e}")
            await self.push_message(dont_throw=True, msg=Message(
                event_name='error', user_id=user_id,
                ref_video_hash=data.get('video_hash'),
                message=f"Failed to lookup video.", details=str(e)))


    async def del_video(self, data: dict, ws: web.WebSocketResponse, user_id: str, username: str):
        try:
            video_hash = str(data['video_hash'])
            self.logger.info(f"[{ws.sid}] del_video: user_id='{user_id}' video_hash='{video_hash}'")
            v = await self.db.get_video(video_hash)
            if v is None:
                await self.push_message(Message(
                    event_name='error', user_id=user_id,
                    ref_video_hash=video_hash,
                    message=f"No such video. Cannot delete."))
            else:
                assert user_id in (v.added_by_userid, 'admin'), f"Video '{video_hash}' not owned by you. Cannot delete."
                await self.db.del_video_and_comments(video_hash)
                await self.push_message(persist=True, msg=Message(
                    event_name='ok', user_id=user_id,
                    ref_video_hash=video_hash,
                    message=f"Video deleted.",
                    details=f"Added by {v.added_by_username} ({v.added_by_userid}) on {v.added_time}. Filename was '{v.orig_filename}'"))
        except Exception as e:
            self.logger.exception(f"[{ws.sid}] Exception in del_video for user '{user_id}': {e}")
            await self.push_message(dont_throw=True, persist=True, msg=Message(
                event_name='error', user_id=user_id,
                ref_video_hash=data.get('video_hash'),
                message= f"Failed to delete video.", details=str(e)))


    async def add_comment(self, data: dict, ws: web.WebSocketResponse, user_id: str, username: str):
        try:
            assert user_id and username
            video_hash = str(data['video_hash'])
            self.logger.info(f"[{ws.sid}] add_comment: user_id='{user_id}' video_hash='{video_hash}', msg='{data.get('comment')}'")

            vid = await self.db.get_video(video_hash)
            if vid is None:
                await self.push_message(Message(
                    event_name='error', user_id=user_id,
                    ref_video_hash=video_hash,
                    message=  f"No such video. Cannot comment."))
                return

            # Parse drawing data if present and write to file
            if drawing := data.get('drawing'):
                assert drawing.startswith('data:'), f"Drawing is not a data URI."
                img_uri = DataURI(drawing)
                assert str(img_uri.mimetype) == 'image/webp', f"Invalid mimetype in drawing."
                ext = str(img_uri.mimetype).split('/')[1]
                sha256 = hashlib.sha256(img_uri.data).hexdigest()
                fn = f"{sha256[:16]}.{ext}"
                drawing_path = self.videos_dir / video_hash / 'drawings' / fn
                drawing_path.parent.mkdir(parents=True, exist_ok=True)
                async with aiofiles.open(drawing_path, 'wb') as f:
                    await f.write(img_uri.data)
                drawing = fn

            c = Comment(
                video_hash = video_hash,
                parent_id = data.get('parent_id') or None,
                user_id = user_id,
                username = username,
                comment = data.get('comment') or '',
                timecode = data.get('timecode') or '',
                drawing = drawing or None)

            await self.db.add_comment(c)
            await self._emit_new_comment(c, send_to=video_hash)

        except Exception as e:
            self.logger.exception(f"[{ws.sid}] Exception in add_comment for user '{user_id}': {e}")
            await self.push_message(dont_throw=True, msg=Message(
                event_name='error', user_id=user_id,
                ref_video_hash=str(data.get('video_hash')),
                message=f"Failed to add comment.", details=str(e)))


    async def edit_comment(self, data: dict, ws: web.WebSocketResponse, user_id: str, username: str):
        try:
            assert user_id and username
            comment_id = data['comment_id']
            comment = str(data['comment'])
            self.logger.info(f"[{ws.sid}] edit_comment: user_id='{user_id}' comment_id='{comment_id}', comment='{comment}'")
            
            old = await self.db.get_comment(comment_id)
            video_hash = str(old.video_hash)
            assert user_id in (old.user_id, 'admin'), "You can only edit your own comments"

            await self.db.edit_comment(comment_id, comment)

            await self.emit_cmd('del_comment', {'comment_id': comment_id}, send_to=video_hash)
            c = await self.db.get_comment(comment_id)
            await self._emit_new_comment(c, send_to=video_hash)

        except Exception as e:
            self.logger.exception(f"[{ws.sid}] Exception in edit_comment for user '{user_id}': {e}")
            await self.push_message(dont_throw=True, msg=Message(
                event_name='error', user_id=user_id,
                ref_comment_id=data.get('comment_id'),
                ref_video_hash=str(data.get('video_hash')),
                message=f"Failed to edit comment.", details=str(e)))


    async def del_comment(self, data: dict, ws: web.WebSocketResponse, user_id: str, username: str):
        try:
            assert user_id and username
            comment_id = data['comment_id']
            self.logger.info(f"[{ws.sid}] del_comment: user_id='{user_id}' comment_id='{comment_id}'")

            old = await self.db.get_comment(comment_id)
            video_hash = str(old.video_hash)
            assert user_id in (old.user_id, 'admin'), "You can only delete your own comments"

            all_comm = await self.db.get_video_comments(video_hash)
            for c in all_comm:
                if c.parent_id == comment_id:
                    raise Exception("Can't delete a comment that has replies")
        
            await self.db.del_comment(comment_id)
            await self.emit_cmd('del_comment', {'comment_id': comment_id}, send_to=video_hash)

        except Exception as e:
            # self.logger.exception(f"[{ws.sid}] Exception in del_comment for user '{user_id}': {e}")
            await self.push_message(dont_throw=True, msg=Message(
                event_name='error', user_id=user_id,
                ref_comment_id=data.get('comment_id'),
                message=f"Failed to delete comment.", details=str(e)))


    async def list_my_messages(self, data: dict, ws: web.WebSocketResponse, user_id: str, username: str):
        try:
            assert user_id
            self.logger.info(f"[{ws.sid}] list_my_messages: user_id='{user_id}'")
            msgs = await self.db.get_user_messages(user_id)
            for m in msgs:
                await self.emit_cmd('message', m.to_dict(), send_to=ws)                
                if not m.seen:
                    await self.db.set_message_seen(m.id, True)
        except Exception as e:
            self.logger.exception(f"[{ws.sid}] Exception in list_my_messages for user '{user_id}': {e}")
            # Don't push new error messages to db, as listing them failed.
            await self.emit_cmd("message", Message(
                    event_name='error', user_id=user_id,
                    message=f"Failed to get messages.",
                    details=str(e)
                ).to_dict(), send_to=ws)


    async def logout(self, data: dict, ws: web.WebSocketResponse, user_id: str, username: str):
        self.logger.info(f"[{ws.sid}] logout: user='{user_id}'")
        try:
            await ws.close()
        except Exception as e:
            self.logger.warning(f"[{ws.sid}] Exception in logout (ws.close()) for user '{user_id}': {e}")
            pass




    async def push_message(self, msg: Message, dont_throw=False, persist=False):
        """
        Push a message to the database and emit it to all clients.
        Set dont_throw if this is called from an exception handler.
        """
        if persist:
            try:
                msg = await self.db.add_message(msg) # Also sets id and timestamp
            except Exception as e:
                self.logger.error(f"Exception in push_message while persisting: {e}")
                if not dont_throw:
                    raise
        try:
            await self.emit_cmd("message", msg.to_dict(), send_to=msg.user_id)
        except Exception as e:
            self.logger.error(f"Exception in push_message while emitting: {e}")
            if not dont_throw:
                raise


    def dict_for_video(self, v: Video) -> dict:
        video_url = self.url_base.rstrip('/') + f'/video/{v.video_hash}/' + (
            'video.mp4' if v.recompression_done else
            ('orig/'+urllib.parse.quote(v.orig_filename, safe='')))

        return {
                'orig_filename': v.orig_filename,
                'video_hash': str(v.video_hash),
                'video_url': video_url,
                'fps': str(round(Decimal(v.fps), 3)),  # eg. 23.976
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
        user_id = headers.get('HTTP_X_REMOTE_USER_ID') or headers.get('X-Remote-User-Id') or headers.get('X-REMOTE-USER-ID')
        if not user_id:
            self.logger.warning("No user id found in header X-REMOTE-USER-ID, using 'anonymous'")
        user_name = headers.get('HTTP_X_REMOTE_USER_NAME') or headers.get('X-Remote-User-Name') or headers.get('X-REMOTE-USER-NAME')
        return (user_id or 'anonymous', user_name or 'Anonymous')


async def run_server(
        db: Database,
        logger: logging.Logger,
        url_base: str,
        msgs_to_push: asyncio.Queue,
        videos_dir: Path,
        upload_dir: Path,
        host='127.0.0.1',
        port: int=8086,
        serve_dirs: dict[str, Path] = {},
        has_started = asyncio.Event(),
        ingest_callback: Callable[Path, str] = None,
    ) -> bool:
    """
    Run HTTP / Websocket API server forever (until this asyncio task is cancelled)

    Params:
        db:           Database object
        logger:       Logger instance for API server
        url_base:     Base URL for the server (e.g. https://example.com). Used e.g. to construct video URLs.
        push_messages: Queue for messages to be pushed to clients
        videos_dir:   Directory where videos are stored
        upload_dir:   Directory where uploaded files are stored
        host:         Hostname to listen on
        port:         Port to listen on
        serve_dirs:   Dict of {route: path} for static file serving
        has_started:  Event that is set when the server has started
        ingest_callback: Callback function to be called when a file is uploaded. Signature: (path: Path, user_id: str) -> None


    Returns:
        True if server was started successfully, False if not.
    """    
    try:
        async with db:
            if db.error_state:
                logger.fatal(f"DB ERROR: {db.error_state}")
                return False

            server = ClapshotApiServer(db=db, url_base=url_base, port=port, logger=logger, videos_dir=videos_dir, upload_dir=upload_dir, serve_dirs=serve_dirs, ingest_callback=ingest_callback)
            runner = web.AppRunner(server.app)
            await runner.setup()
            # bind to localhost only, no matter what url_base is (for security, use reverse proxy to expose)
            logger.info(f"Starting API server. Binding to {host}:{server.port} -- Base URL: {url_base}")
            site = web.TCPSite(runner, host, server.port)
            await site.start()
            has_started.set()

            # Wait for messages from other parts of the app,
            # and push them to clients.
            while msg := await msgs_to_push.get():
                await server.push_message(msg, persist=True, dont_throw=True)

    except Exception as e:
        logger.error(f"Exception in API server: {e}")
        return False

    return True
