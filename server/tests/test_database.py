from pytest_cov.embed import cleanup_on_sigterm
cleanup_on_sigterm()

import pytest, random
from pathlib import Path
from clapshot_server import database as DB

"""
Database tests.
"""

random.seed()
import logging
logging.basicConfig(level=logging.INFO)


# Simple test database test against
@pytest.fixture
async def example_db(tmp_path_factory):
    """
    <Video(id=1 video_hash=HASH0 orig_filename=test0.mp4 added_by_userid=user.num1 ...)>
    <Video(id=2 video_hash=HASH1 orig_filename=test1.mp4 added_by_userid=user.num2 ...)>
    <Video(id=3 video_hash=HASH2 orig_filename=test2.mp4 added_by_userid=user.num1 ...)>
    <Video(id=4 video_hash=HASH3 orig_filename=test3.mp4 added_by_userid=user.num2 ...)>
    <Video(id=5 video_hash=HASH4 orig_filename=test4.mp4 added_by_userid=user.num1 ...)>
    <Comment(id='1' video=HASH0 parent=None user_id='user.num1' comment='Comment 0' has-drawing=True ...)>
    <Comment(id='2' video=HASH1 parent=None user_id='user.num2' comment='Comment 1' has-drawing=True ...)>
    <Comment(id='3' video=HASH2 parent=None user_id='user.num1' comment='Comment 2' has-drawing=True ...)>
    <Comment(id='4' video=HASH0 parent=None user_id='user.num2' comment='Comment 3' has-drawing=True ...)>
    <Comment(id='5' video=HASH1 parent=None user_id='user.num1' comment='Comment 4' has-drawing=True ...)>
    <Comment(id='6' video=HASH0 parent=1 user_id='user.num2' comment='Comment 5' has-drawing=True ...)>
    <Comment(id='7' video=HASH0 parent=1 user_id='user.num1' comment='Comment 6' has-drawing=True ...)>
    """
    dst_dir = tmp_path_factory.mktemp("clapshot_database_test")
    db_file = Path(dst_dir / "test.sqlite")

    Path(db_file).unlink(missing_ok=True)
    async with DB.Database(Path(db_file), logging.getLogger()) as db:
        assert not db.error_state, f"DB error state {db.error_state}"
        async def mkvid(i):
            v = DB.Video(
                video_hash=f"HASH{i}",
                added_by_userid=f"user.num{1+i%2}",
                added_by_username=f"User Number{1+i%2}",
                orig_filename=f"test{i}.mp4",
                total_frames=i*1000,
                duration=i*100,
                fps=i*i,
                raw_metadata_all="{all: {video:" + str(i) + "}}")
            v.id = await db.add_video(v)
            return v

        videos = [await mkvid(i) for i in range(5)]

        async def mkcom(i, video_hash, parent_id=None):
            c = DB.Comment(
                video_hash=video_hash,
                parent_id=parent_id,
                user_id=f"user.num{1+i%2}",
                username=f"User Number{1+i%2}",
                comment=f"Comment {i}",
                drawing=f"drawing_{i}.webp")
            c.id = await db.add_comment(c)

            dp = Path(dst_dir / 'videos' / video_hash / 'drawings')
            dp.mkdir(parents=True, exist_ok=True)
            (dp / c.drawing).write_text("IMAGE_DATA")

            return c
        
        comments = [await mkcom(i, videos[i%3].video_hash) for i in range(5)]
        comments.extend([await mkcom(i, comments[0].video_hash, parent_id=comments[0].id) for i in range(5,5+2)])

        try:
            yield (db, videos, comments)
        finally:
            Path(db_file).unlink(missing_ok=True)


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_fixture_state(example_db):
    async for (db, vid, com) in example_db:
        # First 5 comments have no parent, last 2 have parent_id=1
        for i in range(5):
            assert com[i].parent_id is None
        for i in range(5,5+2):
            assert com[i].parent_id == com[0].id

        # to_dict() and repr should work
        for x in (vid + com):
            assert x.to_dict()
            assert str(x)

        # Video #0 has 3 comments, video #1 has 2, video #2 has 1
        assert com[0].video_hash == com[3].video_hash == com[5].video_hash == com[6].video_hash == (vid[0].video_hash)
        assert com[1].video_hash == com[4].video_hash == (vid[1].video_hash)
        assert com[2].video_hash == (vid[2].video_hash)

        # Read entries from database and check that they match definitions
        for v in vid:
            assert (await db.get_video(v.video_hash)).video_hash == v.video_hash
            comments = await db.get_video_comments(v.video_hash)
            assert len(comments) == {'HASH0': 4, 'HASH1': 2, 'HASH2': 1, 'HASH3': 0, 'HASH4': 0}[v.video_hash]    
        for c in com:
            assert (await db.get_comment(c.id)).id == c.id
            assert (await db.get_comment(c.id)).comment == c.comment

        # Check that we can get videos by user
        assert len(await db.get_all_user_videos('user.num1')) == 3
        assert len(await db.get_all_user_videos('user.num2')) == 2


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_comment_edit(example_db):
    async for (db, vid, com) in example_db:
        # Edit one commment and check that it was edited, and nothing else
        new_comment = "New comment"
        await db.edit_comment(com[2].id, new_comment)
        for c in com:
            if c.id == com[2].id:
                assert (await db.get_comment(c.id)).comment == new_comment
            else:
                assert (await db.get_comment(c.id)).comment != new_comment


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_video_delete(example_db):
    async for (db, vid, com) in example_db:
        await db.del_video_and_comments(vid[1].video_hash)
        for v in vid:
            if v.video_hash == vid[1].video_hash:
                assert (await db.get_video(v.video_hash)) is None, "Video should be deleted"
            else:
                assert (await db.get_video(v.video_hash)).video_hash == v.video_hash, "Deletion removed wrong video(s)"

        # Check that comments were deleted as well
        assert len(await db.get_video_comments(vid[1].video_hash)) == 0, "Comments were not deleted when video was deleted"


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_comment_delete(example_db):
    async for (db, vid, com) in example_db:
        assert len(await db.get_video_comments(com[1].video_hash)) == 2, "Video should have 2 comments before deletion"

        # Delete comment #2 and check that it was deleted, and nothing else
        await db.del_comment(com[1].id)
        for c in com:
            if c.id == com[1].id:
                assert (await db.get_comment(c.id)) is None, "Comment should be deleted"
            else:
                assert (await db.get_comment(c.id)).id == c.id, "Deletion removed wrong comment(s)"

        # Check that video still has 1 comment
        assert len(await db.get_video_comments(com[1].video_hash)) == 1, "Video should have 1 comment left"

        # Delete last, add a new one and check for ID reuse
        await db.del_comment(com[6].id)
        c = DB.Comment(video_hash=com[1].video_hash, user_id=com[1].user_id, username=f"name", comment=f"re-add")
        new_id = await db.add_comment(c)
        assert new_id != com[6].id, "Comment ID was re-used after deletion. This would mix up comment threads in the UI."


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_repr(example_db):
    async for (db, vid, com) in example_db:
        for v in vid:        
            assert v.video_hash in repr(v)
            assert v.orig_filename in repr(v)    
            assert v.video_hash == v.to_dict()['video_hash']
        for c in com:
            assert c.video_hash in repr(c)
            assert c.comment in repr(c)
            assert c.username == c.to_dict()['username']


@pytest.mark.timeout(15)
@pytest.mark.asyncio
async def test_user_messages(example_db):
    async for (db, vid, com) in example_db:
        # Add a message to user #1
        msgs = [
            DB.Message(user_id='user.num1', message='message1', event_name="info", ref_video_hash="HASH0"),
            DB.Message(user_id='user.num1', message='message2', event_name="oops", ref_video_hash="HASH0", details="STACKTRACE"),
            DB.Message(user_id='user.num2', message='message3', event_name="info")
        ]

        for i,m in enumerate(msgs):
            new_msg = await db.add_message(m)
            assert new_msg.to_dict()
            assert str(new_msg)
            msgs[i].id = new_msg.id
            assert new_msg.created
            got = await db.get_message(msgs[i].id)
            assert got.to_dict() == msgs[i].to_dict()
            assert not got.seen

        # Correctly count messages
        assert len(await db.get_user_messages('user.num1')) == 2
        assert len(await db.get_user_messages('user.num2')) == 1

        # Mark message #2 as seen
        await db.set_message_seen(msgs[1].id, True)
        assert (await db.get_message(msgs[1].id)).seen

        # Delete & recount
        await db.del_message(msgs[2].id)
        await db.del_message(msgs[0].id)
        assert len(await db.get_user_messages('user.num1')) == 1
        assert len(await db.get_user_messages('user.num2')) == 0
