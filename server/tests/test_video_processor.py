import asyncio
from datetime import datetime
from fractions import Fraction
import json
import queue
import shutil
import time
from types import SimpleNamespace
from pytest_cov.embed import cleanup_on_sigterm
cleanup_on_sigterm()

import threading

from contextlib import suppress
import pytest, random
from pathlib import Path
import logging

from clapshot_server.video_processor import VideoProcessor, ProcessingResult
from clapshot_server.database import Database


random.seed()
logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)

SRC_VIDEO="tests/assets/NASA_Red_Lettuce_excerpt.mov"


@pytest.fixture()
def temp_dir(tmp_path_factory):
    # Example video
    assert Path(SRC_VIDEO).exists(), f"Test video '{src.absolute}' does not exist"
    dst_dir = tmp_path_factory.mktemp("clapshot_videoproc_test")
    shutil.copy2(SRC_VIDEO, dst_dir)

    # Invalid video file
    src_garbage = dst_dir / "garbage.mov"
    with open(src_garbage, 'wb') as f:
        f.write(b'das ist kein video')

    src = Path(dst_dir / Path(SRC_VIDEO).name)
    assert src.exists(), f"Test video '{src.absolute}' does not exist"
    vp = VideoProcessor(dst_dir/"test.sqlite", logger)
    try:
        yield [(src, dst_dir, src_garbage, vp)]
    finally:        
        shutil.rmtree(dst_dir, ignore_errors=True)


@pytest.mark.slow
@pytest.mark.timeout(15)
def test_convert_hbr(temp_dir):
    """
    Test converting a hight bitrate video to a smaller one.
    """
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        dst_file = dst_dir / "test.mp4"
        assert not dst_file.exists()
        f_stdout, f_stderr = vp.convert_video(src, dst_file, logger, 15*10**6, 'fake-codec')
        assert dst_file.exists()
        assert dst_file.stat().st_size < src.stat().st_size
        assert f_stdout.exists()
        assert f_stderr.exists()


@pytest.mark.timeout(15)
def test_no_conv_smaller_mp4(temp_dir):
    """
    Test that a smaller h264 MP¤ file is not converted, only copied.
    """
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        dst_file = dst_dir / "test.mp4"
        assert not dst_file.exists()
        src_mp4 = dst_dir / "src.mp4"
        shutil.copy(src, src_mp4)
        assert src_mp4.exists()
        res_tuple = vp.convert_video(Path(src_mp4), dst_file, logger, 1*10**6, 'h264')
        assert res_tuple is None
        assert dst_file.exists()
        assert dst_file.stat().st_size == src.stat().st_size
        assert not (dst_dir / "encoder.stdout").exists()
        assert not (dst_dir / "encoder.stderr").exists()


@pytest.mark.slow
@pytest.mark.timeout(15)
def test_conv_smaller_mov(temp_dir):
    """
    Test that a smaller MOV file is converted to MP4, even if it's (supposedly) h264 and low bitrate.
    """
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        dst_file = dst_dir / "test.mp4"
        assert not dst_file.exists()
        vp.convert_video(Path(src), dst_file, logger, 1*10**6, 'h264')
        assert dst_file.exists()


@pytest.mark.timeout(15)
def test_conversion_error(temp_dir):
    """
    Test that a conversion error is handled correctly.
    """
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        dst_file = dst_dir / "test.mp4"

        # Exception should be raised and error log written
        with pytest.raises(Exception) as e_info:
            vp.convert_video(src_garbage, dst_file, logger, 123456, 'h264')
        assert (dst_dir / "encoder.stderr").stat().st_size > 0, "stderr file is empty"



def _get_video_from_db(video_hash: str, db_file: Path):
    async def _do_get():
        async with Database(Path(db_file), logger) as db:
            return await db.get_video(video_hash)
    return asyncio.run(_do_get())


@pytest.mark.slow
@pytest.mark.timeout(15)
def test_read_metadata_ok(temp_dir):
    """
    Test that metadata is read correctly.
    """
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        vid = _get_video_from_db("testhash1", vp.db_file)
        assert vid is None

        dst_file = dst_dir / "test.mp4"
        res, orig_codec, orig_bitrate = vp.read_video_metadata(src, "testhash1", logger, lambda e,s: (e, s))
        assert res is None, "Got error message"
        
        vp.convert_video(src, dst_file, logger, 1*10**6, 'hevc')

        res, new_codec, new_bitrate = vp.read_video_metadata(dst_file, "testhash2", logger, lambda e,s: (e, s))
        assert res is None, "Got error message"
        assert orig_codec == 'hevc'
        assert new_codec == 'h264'
        assert orig_bitrate >= new_bitrate

        vid = _get_video_from_db("testhash1", vp.db_file)
        assert vid.orig_filename == src.name
        meta_video = json.loads(vid.raw_metadata_video)
        assert meta_video['codec_name'] == orig_codec
        assert int(meta_video['bit_rate']) == orig_bitrate
        assert float(Fraction(meta_video['avg_frame_rate'])) == pytest.approx(float(vid.fps), 0.1)


@pytest.mark.timeout(15)
def test_fail_read_metadata_no_video_stream(temp_dir):
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        res, codec, bitrate = vp.read_video_metadata(src, "testhash", logger, lambda e,s: (e, s), test_mock={'no_video_stream': True})
        assert res is not None and res[1] is False

@pytest.mark.timeout(15)
def test_metadata_no_explicit_fps(temp_dir):
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        res, codec, bitrate = vp.read_video_metadata(src, "testhash", logger, lambda e,s: (e, s), test_mock={'no_fps': True})
        assert res is None
        vid = _get_video_from_db("testhash", vp.db_file)
        meta_video = json.loads(vid.raw_metadata_video)
        assert float(Fraction(meta_video['avg_frame_rate'])) == pytest.approx(float(vid.fps), 0.1)


@pytest.mark.timeout(15)
def test_fail_read_metadata(temp_dir):
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        vid = _get_video_from_db("testhash1", vp.db_file)
        assert vid is None
        res, codec, bitrate = vp.read_video_metadata(src_garbage, "testhash", logger, lambda e,s: (e, s))
        assert res is not None and res[1] is False
        assert _get_video_from_db("testhash", vp.db_file) is None


@pytest.mark.timeout(15)
def test_fail_read_metadata_bad_db(temp_dir):
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        vp.db_file = Path("/dev/null")
        res, codec, bitrate = vp.read_video_metadata(src, "testhash", logger, lambda e,s: (e, s))
        assert res[1] is False  # success = False
        assert "sql" in res[0].lower()  # error
        with pytest.raises(Exception):
            assert _get_video_from_db("testhash", vp.db_file) is None  # Should fail as well


@pytest.mark.slow
@pytest.mark.timeout(15)
def test_process_video_ok(temp_dir):
    """
    Test that a video is processed (both probed and compressed) completely.
    """
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        res = vp.process_file(src, dst_dir)
        assert res.success
        assert res.orig_file.name == src.name
        assert res.video_hash is not None
        assert res.file_owner_id is not None
        assert res.msg is not None
        assert (dst_dir / res.video_hash / "video.mp4").exists()
        assert (dst_dir / res.video_hash / "orig" / src.name).exists()

        vid = _get_video_from_db(res.video_hash, vp.db_file)
        assert vid.orig_filename == src.name
        assert vid.added_by_userid == res.file_owner_id


@pytest.mark.timeout(15)
def test_process_video_corrupted(temp_dir):
    """
    Test that a corrupted video gives a controlled error.
    """    
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        res = vp.process_file(src_garbage, dst_dir)
        print("RES OF test_process_video_corrupted", res)
        assert not res.success
        assert res.orig_file.name == src_garbage.name
        assert res.msg is not None
        
        vid = _get_video_from_db(res.video_hash, vp.db_file)
        assert not vid

@pytest.mark.timeout(15)
def test_process_dev_null_failure(temp_dir):
    """
    Test that a complete broken input gives a controlled error.
    """
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        res = vp.process_file(Path("/dev/null"), dst_dir)
        assert not res.success
        assert res.msg is not None

@pytest.mark.slow
@pytest.mark.timeout(15)
def test_monitor_dir(temp_dir):
    """
    Test incomig video monitoring.
    """
    # vp.monitor_incoming_folder_loop(self, incoming_dir: Path, dst_dir: Path, interrupt_flag: mp.Event):
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        incoming_dir = dst_dir / "incoming"
        incoming_dir.mkdir()

        rejected_dir = dst_dir / "rejected"
        rejected_dir.mkdir()

        videos_dir = dst_dir / "videos"
        videos_dir.mkdir()
        
        def now():
            return datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")

        interrupt_flag = threading.Event()
        result_queue = queue.SimpleQueue()

        print(f"Starting monitor at {now()}...")
        p = threading.Thread(
            target=VideoProcessor.monitor_incoming_folder_loop,
            args=(vp, incoming_dir, videos_dir, rejected_dir, interrupt_flag, result_queue, 0.1, {"test_skip_list": True}))
        p.start()
        time.sleep(1)
        
        print(f"Copying '{src}' to '{incoming_dir}' at {now()}...")
        shutil.copy(src, incoming_dir / src.name)
        shutil.copy(src_garbage, incoming_dir / src_garbage.name)
        time.sleep(1)
        
        print(f"Stopping monitor at {now()}...")
        interrupt_flag.set()
        print(f"Waiting for monitor & children to stop at {now()}...")
        p.join()

        # Check that both ok and corrupted files were processed
        assert not result_queue.empty()
        res_ok = result_queue.get_nowait()
        res_fail = result_queue.get_nowait()
        assert result_queue.empty()

        if res_fail.success:
            res_ok, res_fail = res_fail, res_ok
        print(f"res_ok = {res_ok}")
        print(f"res_fail = {res_fail}")

        assert res_ok.success
        assert res_ok.msg is not None

        vid = _get_video_from_db(res_ok.video_hash, vp.db_file)

        assert res_ok.video_hash
        assert vid.orig_filename == src.name
        assert res_ok.file_owner_id
        assert vid.added_by_userid == res_ok.file_owner_id
        assert src.name in res_ok.__repr__()

        assert (videos_dir / res_ok.video_hash / "video.mp4").exists()
        assert (videos_dir / res_ok.video_hash / "orig" / src.name).exists()

        assert not res_fail.success
        assert res_fail.msg is not None
        assert res_fail.orig_file.name == src_garbage.name
        assert res_fail.file_owner_id is not None
        assert src_garbage.name in res_fail.__repr__()
        assert not (videos_dir / res_fail.video_hash / "orig" / src_garbage.name).exists(), "Cleanup failed - original file was not deleted"
        assert not (videos_dir  / res_fail.video_hash / "video.mp4").exists()
        assert (rejected_dir / res_fail.video_hash / src_garbage.name).exists(), "Cleanup failed. Corrupted file should have been moved to rejected folder"


@pytest.mark.slow
@pytest.mark.timeout(15)
def test_monitor_dir_bad_reject_dir(temp_dir):
    """
    Test incomig video monitoring.
    """
    # vp.monitor_incoming_folder_loop(self, incoming_dir: Path, dst_dir: Path, interrupt_flag: mp.Event):
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        incoming_dir = dst_dir / "incoming"
        incoming_dir.mkdir()
        rejected_dir = "/dev/null"
        videos_dir = dst_dir / "videos"
        videos_dir.mkdir()
        
        interrupt_flag = threading.Event()
        result_queue = queue.SimpleQueue()
        p = threading.Thread(
            target=VideoProcessor.monitor_incoming_folder_loop,
            args=(vp, incoming_dir, videos_dir, rejected_dir, interrupt_flag, result_queue, 0.1))
        p.start()
        time.sleep(1)        
        shutil.copy(src_garbage, incoming_dir / src_garbage.name)
        time.sleep(1)
        
        interrupt_flag.set()
        p.join()

        # Expect only one result        
        assert not result_queue.empty()
        res_fail = result_queue.get_nowait()
        assert result_queue.empty()

        # Must not be added to DB
        assert res_fail.video_hash
        vid = _get_video_from_db(res_fail.video_hash, vp.db_file)
        assert not vid

        assert not res_fail.success
        assert res_fail.orig_file.name == src_garbage.name
        in_orig = videos_dir / res_fail.video_hash / "orig" / src_garbage.name
        assert in_orig.exists(), "Original should've not been removed if rejected folder is not writable"
        assert in_orig.stat().st_size == src_garbage.stat().st_size


@pytest.mark.slow
@pytest.mark.timeout(15)
def test_monitor_dir_bad_video_dir(temp_dir):
    """
    Test incomig video monitoring.
    """
    # vp.monitor_incoming_folder_loop(self, incoming_dir: Path, dst_dir: Path, interrupt_flag: mp.Event):
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        incoming_dir = dst_dir / "incoming"
        incoming_dir.mkdir()
        rejected_dir = dst_dir / "rejected"
        rejected_dir.mkdir()
        videos_dir = Path("/dev/null")
        
        interrupt_flag = threading.Event()
        result_queue = queue.SimpleQueue()
        p = threading.Thread(
            target=VideoProcessor.monitor_incoming_folder_loop,
            args=(vp, incoming_dir, videos_dir, rejected_dir, interrupt_flag, result_queue, 0.1))
        p.start()
        time.sleep(1)        
        shutil.copy(src_garbage, incoming_dir / src_garbage.name)
        time.sleep(1)
        
        interrupt_flag.set()
        p.join()

        # Expect only one result        
        assert not result_queue.empty()
        res_fail = result_queue.get_nowait()
        assert result_queue.empty()

        assert not res_fail.video_hash, "Video should not have been probed if it couldn't be moved to videos folder"
        assert not res_fail.success
        assert res_fail.orig_file.name == src_garbage.name
        assert src_garbage.exists(), "Original should not have been removed if neither video folder nor rejected folder war not writable"


@pytest.mark.slow
@pytest.mark.timeout(15)
def test_monitor_dir_bad_video_and_reject_dir(temp_dir):
    """
    Test incomig video monitoring.
    """
    # vp.monitor_incoming_folder_loop(self, incoming_dir: Path, dst_dir: Path, interrupt_flag: mp.Event):
    for (src, dst_dir, src_garbage, vp) in temp_dir:
        incoming_dir = dst_dir / "incoming"
        incoming_dir.mkdir()
        rejected_dir = Path("/dev/null")
        videos_dir = Path("/dev/null")
        
        interrupt_flag = threading.Event()
        result_queue = queue.SimpleQueue()
        p = threading.Thread(
            target=VideoProcessor.monitor_incoming_folder_loop,
            args=(vp, incoming_dir, videos_dir, rejected_dir, interrupt_flag, result_queue, 0.1))
        p.start()
        time.sleep(1)        
        shutil.copy(src_garbage, incoming_dir / src_garbage.name)
        time.sleep(1)
        
        interrupt_flag.set()
        p.join()

        # Expect only one result        
        assert not result_queue.empty()
        res_fail = result_queue.get_nowait()
        assert result_queue.empty()

        assert not res_fail.video_hash, "Video should not have been probed if it couldn't be moved to videos folder"
        assert not res_fail.success
        assert res_fail.orig_file.name == src_garbage.name
        assert src_garbage.exists(), "Original should not have been removed if neither video folder nor rejected folder war not writable"
