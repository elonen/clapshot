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
from clapshot_server.api_server import run_server
from .test_database import example_db

from datauri import DataURI
import aiohttp

random.seed()
import logging
logging.basicConfig(level=logging.INFO)



@pytest.fixture
async def api_server_and_db(example_db, request):
    async for (db, vid, com) in example_db:
        print("<> Fixture api_server_and_db constructing")
        assert db.error_state is None
        ses, ws = None, None
        try:
            # ----------------------------------------
            # Server
            # ----------------------------------------
            push_msg_queue = asyncio.Queue()

            port = random.randint(10000, 20000)
            started_evt = asyncio.Event()
            server_task = asyncio.create_task(
                run_server(
                    db=db,
                    logger=logging.getLogger("clapshot.db"),
                    url_base='http://127.0.0.1/',
                    port=port,
                    videos_dir=(db.db_file.parent / 'videos'),
                    upload_dir=(db.db_file.parent / 'upload'),
                    msgs_to_push=push_msg_queue,
                    has_started=started_evt),
                name="api_server_and_db--server_task")

            server_task.add_done_callback(lambda t: print("<> Fixture: API server_task done:", t))
            print("<> Fixture waiting for API server to start...")
            await started_evt.wait()
            print("<> Fixture: API server started.")
            
            # ----------------------------------------
            # Client
            # ----------------------------------------
            user_id, username = "user.num1", "User Number1"
            if hasattr(request, 'param'):
                user_id = request.param["user_id"]
                username = request.param["username"]

            timeout = aiohttp.ClientTimeout(total=60)
            ses = aiohttp.ClientSession(timeout=timeout)
            ws = await ses.ws_connect(
                    f'http://127.0.0.1:{port}/api/ws',
                    headers={'HTTP_X_REMOTE_USER_ID': user_id, 'HTTP_X_REMOTE_USER_NAME': username})
            assert ws.closed is False


            async def get_ws_msg(timeout):
                msg = await ws.receive(timeout)
                if msg.type == aiohttp.WSMsgType.TEXT:
                    msg = msg.json()
                    cmd, data = msg['cmd'], msg['data']
                    print(f"<> Fixture: server->client: '{cmd}': {data}")
                    return cmd, data
                elif msg.type == aiohttp.WSMsgType.ERROR:
                    raise msg.data
                elif msg.type == aiohttp.WSMsgType.CLOSED:
                    print("<> Fixture: server->client: CLOSED")
                elif msg.type == aiohttp.WSMsgType.CLOSE:
                    print("<> Fixture: server->client: CLOSE")
                else:
                    raise ValueError(f"Unexpected message type: {msg.type}")


            async def get_msg():
                try:
                    return await get_ws_msg(timeout=0.5)
                except asyncio.TimeoutError:
                    print("<> fixture get_msg timeout. No new messages.")

            async def send(event, data):
                print(f"<> Fixture: client->server: '{event}': {data}")
                await ws.send_json({'cmd': event, 'data': data})

            def break_db():
                db.db_file.unlink()

            @dataclass
            class TestData:
                send: Callable
                get: Callable
                db: DB.Database
                videos: list[DB.Video]
                comments: list[DB.Comment]
                port: int
                break_db: Callable
                pushq: push_msg_queue

            # Consume welcome message
            assert (await get_msg())[0] == 'welcome'

            print("<> Fixture api_server_and_db yielding...")
            yield TestData(send=send, get=get_msg, db=db, videos=vid, comments=com, port=port, break_db=break_db, pushq=push_msg_queue)
            print("<> ...yield returned for Fixture api_server_and_db")
            
        finally:
            print("<> Fixture api_server_and_db cleaning up")
            print("<> Fixture closing ws")
            await ws.close()
            print("<> Fixture closing ses")
            await ses.close()
            print("<> Fixture canceling server_task...")
            server_task.cancel()
            await asyncio.sleep(0)
            with suppress(asyncio.CancelledError):
                await server_task
            await asyncio.sleep(0.1)
            print("<> Fixture api_server_and_db exiting completely")



async def _open_video(td, video_hash):
    """
    Helper function to open a video (which also joins user into video's socketio "room"),
    and to consume all the new_message events that server sends when opening a video.
    """
    await td.send('open_video', {'video_hash': video_hash})
    event, data = await td.get()
    assert event == 'open_video'
    while m := await td.get():
        assert m[0] == 'new_comment'
        assert m[1]['video_hash'] == video_hash
    return event, data



@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_api_push_msq(api_server_and_db):
    async for td in api_server_and_db:

        # Send a message
        td.pushq.put_nowait(DB.Message(user_id="user.num1", event_name="test_push_msg", details="test_param"))
        event, data = await td.get()
        assert event == "message"
        assert data["event_name"] == "test_push_msg"
        assert data["details"] == "test_param"

        # Send to another user, user.num1 should not receive it
        td.pushq.put_nowait(DB.Message(user_id="user.num2", event_name="failure_test"))
        assert not await td.get()


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_api_list_user_videos(api_server_and_db):
    async for td in api_server_and_db:
        await td.send('list_my_videos', {})
        event, data = await td.get()
        print(data)
        assert event == 'user_videos'
        assert data['user_id'] == 'user.num1'
        assert data['username'] == 'User Number1'
        assert len(data['videos']) == 3

        # Break the database       
        td.break_db()
        await td.send('list_my_videos', {})
        event, data = await td.get()
        assert data.get('event_name') == 'error'


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_api_del_video(api_server_and_db):
    async for td in api_server_and_db:

        # Delete a video
        assert await td.db.get_video(td.videos[0].video_hash)
        await td.send('del_video', {'video_hash': td.videos[0].video_hash})
        event, data = await td.get()
        assert data.get('event_name') != 'error'

        assert not await td.db.get_video(td.videos[0].video_hash)

        # Fail to delete a non-existent video
        await td.send('del_video', {'video_hash': 'non-existent'})
        event, data = await td.get()
        assert data.get('event_name') == 'error'

        # Fail to delete someones else's video
        assert await td.db.get_video(td.videos[1].video_hash)
        await td.send('del_video', {'video_hash': td.videos[1].video_hash})
        event, data = await td.get()
        assert data.get('event_name') == 'error'
        assert await td.db.get_video(td.videos[1].video_hash)
        
        # Break the database
        td.break_db()
        await td.send('del_video', {'video_hash': td.videos[2].video_hash})
        event, data = await td.get()
        assert data.get('event_name') == 'error'


@pytest.mark.timeout(15)
@pytest.mark.asyncio
@pytest.mark.parametrize("api_server_and_db", [{'user_id': 'admin', 'username': 'Admin'}], indirect=True)
async def test_api_del_video_as_admin(api_server_and_db):
        async for td in api_server_and_db:
            # Delete to videos by different users
            for vi in (0,1):
                    assert await td.db.get_video(td.videos[vi].video_hash)
                    await td.send('del_video', {'video_hash': td.videos[vi].video_hash})
                    event, data = await td.get()
                    assert data.get('event_name') != 'error'
                    assert not await td.db.get_video(td.videos[vi].video_hash)


@pytest.mark.timeout(20)
@pytest.mark.asyncio
async def test_api_open_videos(api_server_and_db):
    async for td in api_server_and_db:
        for vid in td.videos:
            event, data = await _open_video(td, vid.video_hash)
            assert vid.video_hash in data['video_url'] 
            assert data['user_id'] == vid.added_by_userid
            assert data['username'] == vid.added_by_username
            assert data['orig_filename'] == vid.orig_filename
            assert datetime.datetime.fromisoformat(data['added_time']) not in (None, '')

        # Break the database
        td.break_db()
        await td.send('open_video', {'video_hash': td.videos[0].video_hash})
        event, data = await td.get()
        assert data.get('event_name') == 'error'


@pytest.mark.timeout(20)
@pytest.mark.asyncio
async def test_api_open_bad_video(api_server_and_db):
    async for td in api_server_and_db:
        await td.send('open_video', {'video_hash': 'bad_hash'})
        event, data = await td.get()
        assert data.get('event_name') == 'error'


@pytest.mark.timeout(20)
@pytest.mark.asyncio
async def test_api_add_plain_comment(api_server_and_db):
    async for td in api_server_and_db:
        vid = td.videos[0]
        
        await td.send('add_comment', {'video_hash': vid.video_hash, 'comment': 'Test comment'})

        # No video opened by the client yet, so no response
        assert not await td.get()

        await _open_video(td, vid.video_hash) # Join room
        
        # Add another comment
        drw_data = DataURI.make('image/webp', charset='utf-8', base64=True, data=r'IMAGE_DATA')
        await td.send('add_comment', {'video_hash': vid.video_hash, 'comment': 'Test comment 2', 'drawing': drw_data})
        event, data = await td.get()
        assert event == 'new_comment'
        assert data['comment'] == 'Test comment 2'

        # Stored in database, the the image must be path to a file, not the actual image data
        assert not (await td.db.get_comment(data['comment_id'])).drawing.startswith('data:image')

        assert data['drawing'].startswith("data:image/webp")
        assert DataURI(data['drawing']).text == "IMAGE_DATA"

        # Add a commen to a nonexisting video
        await td.send('add_comment', {'video_hash': 'bad_hash', 'comment': 'Test comment 3'})
        event, data = await td.get()
        assert data.get('event_name') == 'error'

        # Break the database
        td.break_db()
        await td.send('add_comment', {'video_hash': vid.video_hash, 'comment': 'Test comment 4'})
        event, data = await td.get()
        assert data.get('event_name') == 'error'



@pytest.mark.timeout(20)
@pytest.mark.asyncio
async def test_api_edit_comment(api_server_and_db):
    async for td in api_server_and_db:
        vid = td.videos[0]
        com = td.comments[0]
        
        await _open_video(td, vid.video_hash) # Join room

        # Edit comment
        await td.send('edit_comment', {'comment_id': com.id, 'comment': 'Edited comment'})
        event, data = await td.get()
        assert event == 'del_comment'
        assert data['comment_id'] == com.id
        event, data = await td.get()
        assert event == 'new_comment'
        assert data['comment_id'] == com.id
        assert data['comment'] == 'Edited comment'
        assert data['video_hash'] == vid.video_hash

        assert data['drawing'].startswith("data:image/webp")
        assert DataURI(data['drawing']).text == "IMAGE_DATA"

        # Edit nonexisting comment
        await td.send('edit_comment', {'comment_id': '1234566999', 'comment': 'Edited comment 2'})
        event, data = await td.get()
        assert data.get('event_name') == 'error'

        # Try to edit someone else's comment
        await td.send('edit_comment', {'comment_id': td.comments[1].id, 'comment': 'Edited comment 3'})
        event, data = await td.get()
        assert data.get('event_name') == 'error'

        # Break the database
        td.break_db()
        await td.send('edit_comment', {'comment_id': com.id, 'comment': 'Edited comment 4'})
        event, data = await td.get()
        assert data.get('event_name') == 'error'



@pytest.mark.timeout(20)
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

        await _open_video(td, td.videos[0].video_hash) # Join room
        
        # Delete comment[6] (user 1)
        await td.send('del_comment', {'comment_id': td.comments[6].id})
        event, data = await td.get()
        assert event == 'del_comment'
        assert data['comment_id'] == td.comments[6].id

        # Fail to delete nonexisting comment
        await td.send('del_comment', {'comment_id': '1234566999'})
        event, data = await td.get()
        assert data.get('event_name') == 'error'

        # Fail to delete user2's comment[3] (user 2)
        await td.send('del_comment', {'comment_id': td.comments[3].id})
        event, data = await td.get()
        assert data.get('event_name') == 'error'
        print(data)
        assert 'your' in data['details']

        # Fail to delete comment[0] that has replies
        await td.send('del_comment', {'comment_id': td.comments[0].id})
        event, data = await td.get()
        assert data.get('event_name') == 'error'
        assert "repl" in data['details']

        # Delete the last remaining reply comment[5]
        await td.db.del_comment(td.comments[5].id)  # Delete from db directly, to avoid user permission check

        # Try again to delete comment id 1 that should now have no replies
        await td.send('del_comment', {'comment_id': td.comments[0].id})
        event, data = await td.get()
        assert event == 'del_comment'


@pytest.mark.timeout(15)
@pytest.mark.asyncio
@pytest.mark.parametrize("api_server_and_db", [{'user_id': 'admin', 'username': 'Admin'}], indirect=True)
async def test_api_del_comment_as_admin(api_server_and_db):
        async for td in api_server_and_db:
            await _open_video(td, td.videos[0].video_hash) # Join first video room

            # Delete comments by different users
            for i in (5,6):
                await td.send('del_comment', {'comment_id': td.comments[i].id})
                event, data = await td.get()
                print(data)
                assert event == 'del_comment'
                assert data['comment_id'] == td.comments[i].id


@pytest.mark.timeout(15)
@pytest.mark.asyncio
@pytest.mark.parametrize("api_server_and_db", [{'user_id': '', 'username': ''}], indirect=True)
async def test_api_anonymous_user(api_server_and_db):
    async for td in api_server_and_db:
        await _open_video(td, td.videos[0].video_hash) # Join first video room

        # crete comment
        await td.send('add_comment', {'video_hash': td.videos[0].video_hash, 'comment': 'Test comment'})
        event, data = await td.get()
        assert data['user_id'] == 'anonymous'


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_list_my_messages(api_server_and_db):
    async for td in api_server_and_db:

        await td.send('list_my_messages', {})
        assert not await td.get()

        msgs = [
            DB.Message(user_id='user.num1', message='message1', event_name="info", ref_video_hash="HASH0"),
            DB.Message(user_id='user.num1', message='message2', event_name="oops", ref_video_hash="HASH0", details="STACKTRACE"),
            DB.Message(user_id='user.num2', message='message3', event_name="info")
        ]
        for i,m in enumerate(msgs):
            msgs[i].id = await td.db.add_message(m)

        await td.send('list_my_messages', {})
        for m in [m for m in msgs if m.user_id == 'user.num1']:
            event, data = await td.get()
            assert data['event_name'] == m.event_name
            assert not data['seen']
        assert not await td.get()   # No more messages

        # List again, this time messages should be marked "seen"
        await td.send('list_my_messages', {})
        for m in [m for m in msgs if m.user_id == 'user.num1']:
            event, data = await td.get()
            assert data['event_name'] == m.event_name
            assert data['seen']




@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_api_logout(api_server_and_db):
    async for td in api_server_and_db:
        await td.send('logout', None)
        assert not await td.get()
