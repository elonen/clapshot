from ast import Call
import asyncio
from dataclasses import dataclass
import datetime
import shutil
from socket import timeout
from typing import Callable
from pytest_cov.embed import cleanup_on_sigterm
cleanup_on_sigterm()

from contextlib import suppress
import pytest, random
from pathlib import Path

from clapshot_server import database as DB
from clapshot_server.api_server import SOCKET_IO_PATH, run_server as run_api_server
from .test_database import example_db

import socketio


random.seed()
import logging
logging.basicConfig(level=logging.INFO)



@pytest.fixture
async def api_server_and_db(example_db, request):
    async for (db, vid, com) in example_db:
        try:
            # ----------------------------------------
            # Server
            # ----------------------------------------
            port = random.randint(10000, 20000)
            server_task = asyncio.create_task(
                run_api_server(
                    db=db,
                    logger=logging.getLogger("clapshot.db"),
                    url_base='http://localhost/',
                    port=port))

            server_task.add_done_callback(lambda t: print("Server task done:", t))
            await asyncio.sleep(0.1)
            
            # ----------------------------------------
            # Client
            # ----------------------------------------
            recvq = asyncio.Queue()
            sio = socketio.AsyncClient()
            user_id, username = "user.num1", "User Number1"
            if hasattr(request, 'param'):
                user_id = request.param["user_id"]
                username = request.param["username"]

            await sio.connect(
                url = f'http://localhost:{port}', 
                socketio_path = SOCKET_IO_PATH,
                headers={'X-REMOTE-USER-ID': user_id, 'X-REMOTE-USER-NAME': username})

            # Message I/O
            @sio.on('*')
            async def catch_all(event, data):
                recvq.put_nowait((event, data))
            async def get_msg():
                return await asyncio.wait_for(recvq.get(), timeout=0.5)
            async def send(event, data):
                await sio.emit(event, data)

            async def break_db():
                db.db_file.unlink()

            @dataclass
            class TestData:
                send: Callable
                get: Callable
                recvq: asyncio.Queue
                db: DB.Database
                videos: list[DB.Video]
                comments: list[DB.Comment]
                port: int
                break_db: Callable

            yield TestData(send=send, get=get_msg, recvq=recvq, db=db, videos=vid, comments=com, port=port, break_db=break_db)
            
        finally:
            server_task.cancel()
            with suppress(asyncio.CancelledError):
                await server_task



@pytest.mark.timeout(5)
@pytest.mark.asyncio
async def test_api_list_user_videos(api_server_and_db):
    async for td in api_server_and_db:
        await td.send('list_my_videos', {})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        print(data)
        assert event == 'user_videos'
        assert data['user_id'] == 'user.num1'
        assert data['username'] == 'User Number1'
        assert len(data['videos']) == 3

        # Break the database       
        await td.break_db()
        await td.send('list_my_videos', {})
        event, data = await td.get()
        assert event == 'error'


@pytest.mark.timeout(5)
@pytest.mark.asyncio
async def test_api_del_video(api_server_and_db):
    async for td in api_server_and_db:

        # Delete a video
        assert await td.db.get_video(td.videos[0].video_hash)
        await td.send('del_video', {'video_hash': td.videos[0].video_hash})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event != 'error'        
        assert not await td.db.get_video(td.videos[0].video_hash)

        # Fail to delete a non-existent video
        await td.send('del_video', {'video_hash': 'non-existent'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'        

        # Fail to delete someones else's video
        assert await td.db.get_video(td.videos[1].video_hash)
        await td.send('del_video', {'video_hash': td.videos[1].video_hash})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'
        assert await td.db.get_video(td.videos[1].video_hash)
        
        # Break the database
        await td.break_db()
        await td.send('del_video', {'video_hash': td.videos[2].video_hash})
        event, data = await td.get()
        print(data)
        assert event == 'error'


@pytest.mark.timeout(5)
@pytest.mark.asyncio
@pytest.mark.parametrize("api_server_and_db", [{'user_id': 'admin', 'username': 'Admin'}], indirect=True)
async def test_api_del_video_as_admin(api_server_and_db):
        async for td in api_server_and_db:
            # Delete to videos by different users
            for vi in (0,1):
                    assert await td.db.get_video(td.videos[vi].video_hash)
                    await td.send('del_video', {'video_hash': td.videos[vi].video_hash})
                    await asyncio.sleep(0.1)
                    event, data = await td.get()
                    assert event != 'error'        
                    assert not await td.db.get_video(td.videos[vi].video_hash)


@pytest.mark.timeout(8)
@pytest.mark.asyncio
async def test_api_open_videos(api_server_and_db):
    async for td in api_server_and_db:
        for vid in td.videos:
            await td.send('open_video', {'video_hash': vid.video_hash})
            await asyncio.sleep(0.1)
            event, data = await td.get()
            assert event == 'open_video'
            assert data['video_hash'] == vid.video_hash
            assert vid.video_hash in data['video_url'] 
            assert data['user_id'] == vid.added_by_userid
            assert data['username'] == vid.added_by_username
            assert data['orig_filename'] == vid.orig_filename
            assert datetime.datetime.fromisoformat(data['added_time']) not in (None, '')

        # Break the database
        await td.break_db()
        await td.send('open_video', {'video_hash': td.videos[0].video_hash})
        event, data = await td.get()
        assert event == 'error'


@pytest.mark.timeout(8)
@pytest.mark.asyncio
async def test_api_open_bad_video(api_server_and_db):
    async for td in api_server_and_db:
        await td.send('open_video', {'video_hash': 'bad_hash'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'


@pytest.mark.timeout(8)
@pytest.mark.asyncio
async def test_api_add_plain_comment(api_server_and_db):
    async for td in api_server_and_db:
        vid = td.videos[0]
        
        await td.send('add_comment', {'video_hash': vid.video_hash, 'comment': 'Test comment'})
        await asyncio.sleep(0.1)

        # Not joined to video's Socket.IO "room" yet, so no response
        with pytest.raises(asyncio.exceptions.TimeoutError) as e_info:
            await td.get()

        # Join room
        await td.send('open_video', {'video_hash': vid.video_hash})
        event, data = await td.get()
        assert event == 'open_video'
        
        # Add another comment
        await td.send('add_comment', {'video_hash': vid.video_hash, 'comment': 'Test comment 2'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'new_comment'
        assert data['comment'] == 'Test comment 2'

        # Add a commen to a nonexisting video
        await td.send('add_comment', {'video_hash': 'bad_hash', 'comment': 'Test comment 3'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'

        # Break the database
        await td.break_db()
        await td.send('add_comment', {'video_hash': vid.video_hash, 'comment': 'Test comment 4'})
        event, data = await td.get()
        assert event == 'error'


@pytest.mark.timeout(8)
@pytest.mark.asyncio
async def test_api_edit_comment(api_server_and_db):
    async for td in api_server_and_db:
        vid = td.videos[0]
        com = td.comments[0]
        
        # Join room
        await td.send('open_video', {'video_hash': vid.video_hash})
        event, data = await td.get()
        assert event == 'open_video'
        
        # Edit comment
        await td.send('edit_comment', {'comment_id': com.id, 'comment': 'Edited comment'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'del_comment'
        assert data['comment_id'] == com.id
        event, data = await td.get()
        assert event == 'new_comment'
        assert data['comment_id'] == com.id
        assert data['comment'] == 'Edited comment'

        # Edit nonexisting comment
        await td.send('edit_comment', {'comment_id': '1234566999', 'comment': 'Edited comment 2'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'

        # Try to edit someone else's comment
        await td.send('edit_comment', {'comment_id': td.comments[1].id, 'comment': 'Edited comment 3'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'

        # Break the database
        await td.break_db()
        await td.send('edit_comment', {'comment_id': com.id, 'comment': 'Edited comment 4'})
        event, data = await td.get()
        assert event == 'error'



@pytest.mark.timeout(8)
@pytest.mark.asyncio
async def test_api_del_comment(api_server_and_db):
    async for td in api_server_and_db:

        # Summary of comment thread used in this test:
        #
        #   video[0]:
        #     comment[0] (user 1)
        #       comment[5] (user 2)
        #       comment[6] (user 1)
        #     comment[3] (user 2)
        
        # Join room for video HASH0
        await td.send('open_video', {'video_hash': td.videos[0].video_hash})
        event, data = await td.get()
        assert event == 'open_video'
        
        # Delete comment[6] (user 1)
        await td.send('del_comment', {'comment_id': td.comments[6].id})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'del_comment'
        assert data['comment_id'] == td.comments[6].id

        # Fail to delete nonexisting comment
        await td.send('del_comment', {'comment_id': '1234566999'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'

        # Fail to delete user2's comment[3] (user 2)
        await td.send('del_comment', {'comment_id': td.comments[3].id})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'
        assert 'your' in data['msg']

        # Fail to delete comment[0] that has replies
        await td.send('del_comment', {'comment_id': td.comments[0].id})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'error'
        assert "repl" in data['msg']

        # Delete the last remaining reply comment[5]
        await td.db.del_comment(td.comments[5].id)  # Delete from db directly, to avoid user permission check

        # Try again to delete comment id 1 that should now have no replies
        await td.send('del_comment', {'comment_id': td.comments[0].id})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert event == 'del_comment'


@pytest.mark.timeout(5)
@pytest.mark.asyncio
@pytest.mark.parametrize("api_server_and_db", [{'user_id': 'admin', 'username': 'Admin'}], indirect=True)
async def test_api_del_comment_as_admin(api_server_and_db):
        async for td in api_server_and_db:

            # Join room for video HASH0
            await td.send('open_video', {'video_hash': td.videos[0].video_hash})
            event, data = await td.get()
            assert event == 'open_video'

            # Delete comments by different users
            for i in (5,6):
                await td.send('del_comment', {'comment_id': td.comments[i].id})
                await asyncio.sleep(0.1)
                event, data = await td.get()
                print(data)
                assert event == 'del_comment'
                assert data['comment_id'] == td.comments[i].id


@pytest.mark.timeout(5)
@pytest.mark.asyncio
@pytest.mark.parametrize("api_server_and_db", [{'user_id': '', 'username': ''}], indirect=True)
async def test_api_anonymous_user(api_server_and_db):
    async for td in api_server_and_db:

        # Join room for video HASH0
        await td.send('open_video', {'video_hash': td.videos[0].video_hash})
        event, data = await td.get()
        assert event == 'open_video'

        # crete comment
        await td.send('add_comment', {'video_hash': td.videos[0].video_hash, 'comment': 'Test comment'})
        await asyncio.sleep(0.1)
        event, data = await td.get()
        assert data['user_id'] == 'anonymous'


@pytest.mark.timeout(8)
@pytest.mark.asyncio
async def test_api_logout(api_server_and_db):
    async for td in api_server_and_db:
        await td.send('logout', {'a': 'b'})
        await asyncio.sleep(0.1)
        with pytest.raises(asyncio.exceptions.TimeoutError) as e_info:
            await td.get()

