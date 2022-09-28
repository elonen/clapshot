import asyncio
from datetime import datetime
from fractions import Fraction
import multiprocessing
from multiprocessing import Process
from pprint import pprint
import queue
import shutil
import time
from typing import Callable
from pytest_cov.embed import cleanup_on_sigterm
cleanup_on_sigterm()

import threading

import pytest, random
from pathlib import Path
import logging

from clapshot_server.database import Database
from clapshot_server import video_ingesting
from clapshot_server import video_metadata_reader
from clapshot_server import video_compressor
from clapshot_server.incoming_monitor import monitor_incoming_folder_loop
from clapshot_server.video_processing_pipeline import VideoProcessingPipeline


random.seed()
logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)

SRC_VIDEO_HBR="tests/assets/NASA_Red_Lettuce_excerpt.mov"
SRC_VIDEO_LBR="tests/assets/60fps-example.mp4"


@pytest.fixture()
def temp_dir(tmp_path_factory):
    # Example video
    assert Path(SRC_VIDEO_HBR).exists(), f"Test video '{src.absolute}' does not exist"
    dst_dir = tmp_path_factory.mktemp("clapshot_videoproc_test")
    
    shutil.copy2(SRC_VIDEO_HBR, dst_dir)
    shutil.copy2(SRC_VIDEO_LBR, dst_dir)

    for d in ["incoming", "rejected", "videos"]:
        (dst_dir / d).mkdir(exist_ok=True)

    # Invalid video file
    src_garbage = dst_dir / "garbage.mov"
    with open(src_garbage, 'wb') as f:
        f.write(b'das ist kein video')

    src = Path(dst_dir / Path(SRC_VIDEO_HBR).name)
    assert src.exists(), f"Test video '{src.absolute}' does not exist"
    src_lbr = Path(dst_dir / Path(SRC_VIDEO_LBR).name)
    assert src_lbr.exists(), f"Test video '{src_lbr.absolute}' does not exist"
    db_file = dst_dir / "test.sqlite"

    try:
        yield [(src, dst_dir, src_garbage, src_lbr, db_file)]
    finally:        
        shutil.rmtree(dst_dir, ignore_errors=True)


@pytest.mark.slow
@pytest.mark.timeout(120)
def test_recompress_ok(temp_dir):
    """
    Test converting a hight bitrate video to a smaller one.
    """
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:

        dst_file = Path(dst_dir) / "test.mp4"
        assert not dst_file.exists()

        with multiprocessing.Manager() as mgt:
            inq, outq = mgt.Queue(), mgt.Queue()
            res = video_compressor.CompressorPool(inq, outq, max_workers=1).compress(
                video_compressor.Args(src, dst_file, 1*1024**2, 'HASH123'), '')
            assert res.success
            assert str(Path(res.dst_file).absolute) == str(Path(dst_file).absolute)
            assert Path(res.dst_file).stat().st_size > 1000
            assert Path(res.dst_file).stat().st_size < Path(src).stat().st_size
            assert res.video_hash == 'HASH123'


@pytest.mark.timeout(120)
def test_fail_recompress_garbage(temp_dir):
    """
    Test that a conversion error is handled correctly.
    """
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        dst_file = dst_dir / "test.mp4"

        with multiprocessing.Manager() as mgt:
            inq, outq = mgt.Queue(), mgt.Queue()
            res = video_compressor.CompressorPool(inq, outq, max_workers=1).compress(
                video_compressor.Args(src_garbage, dst_file, 1*1024**2, "HASH123"), '')

            assert not res.success
            assert res.msg
            assert len(res.stderr) > 0


def _get_video_from_db(video_hash: str, db_file: Path):
    async def _do_get():
        async with Database(Path(db_file), logger) as db:
            assert not db.error_state, f"DB error state {db.error_state}"
            return await db.get_video(video_hash)
    return asyncio.run(_do_get())

def _wait_get(queue: multiprocessing.Queue, timeout: float = 1.0):
    start = time.time()
    while time.time() - start < timeout:
        if not queue.empty():
            return queue.get()
        time.sleep(0.1)
    return None


@pytest.mark.timeout(120)
def test_read_metadata_ok(temp_dir):
    """
    Test that metadata is read correctly.
    """
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        vid = _get_video_from_db("testhash1", db_file)
        assert vid is None
        dst_file = dst_dir / "test.mp4"

        with multiprocessing.Manager() as mgt:
            inq, outq = mgt.Queue(), mgt.Queue()
            res = video_metadata_reader.ReaderPool(inq, outq, max_workers=1).read_metadata(
                video_metadata_reader.Args(src, user_id='test-user'), '')
            assert res.success
            assert res.total_frames == 123
            assert float(Fraction(res.fps)) == pytest.approx(float(23.976), 0.01)
            assert float(res.duration) == pytest.approx(float(5.13), 0.1)
            assert res.orig_codec.lower() in ('hevc', 'h265')
            assert len(res.metadata_all.keys()) > 0
            assert res.user_id == 'test-user'


@pytest.mark.timeout(120)
def test_fail_read_metadata_no_video_stream(temp_dir):
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        with multiprocessing.Manager() as mgt:
            inq, outq = mgt.Queue(), mgt.Queue()
            res = video_metadata_reader.ReaderPool(inq, outq, max_workers=1).read_metadata(
                video_metadata_reader.Args(src, test_mock={'no_video_stream': True}), '')
            assert not res.success
            assert res.msg

@pytest.mark.timeout(120)
def test_fail_read_metadata_missing_fields(temp_dir):
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        with multiprocessing.Manager() as mgt:
            inq, outq = mgt.Queue(), mgt.Queue()
            res = video_metadata_reader.ReaderPool(inq, outq, max_workers=1).read_metadata(
                video_metadata_reader.Args(src, test_mock={'missing_mediainfo_fields': True}), '')
            assert not res.success
            assert res.msg

@pytest.mark.timeout(120)
def test_read_metadata_approximate_bitrate(temp_dir):
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        with multiprocessing.Manager() as mgt:
            inq, outq = mgt.Queue(), mgt.Queue()
            res = video_metadata_reader.ReaderPool(inq, outq, max_workers=1).read_metadata(
                video_metadata_reader.Args(src, test_mock={'no_bit_rate': True}), '')
            assert res.success
            assert 8000000 < res.bitrate < 9000000


@pytest.mark.timeout(120)
def test_fail_read_metadata_garbage(temp_dir):
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        with multiprocessing.Manager() as mgt:
            inq, outq = mgt.Queue(), mgt.Queue()
            res = video_metadata_reader.ReaderPool(inq, outq, max_workers=1).read_metadata(
                video_metadata_reader.Args(src_garbage), '')
            assert not res.success
            assert res.msg


@pytest.mark.slow
@pytest.mark.timeout(240)
def test_incoming_monitor(temp_dir):
    """
    Test incomig video monitoring.
    """
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        with multiprocessing.Manager() as mgt:
            incoming_dir = dst_dir / "incoming"
            
            def now():
                return datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")

            submit_q = multiprocessing.Queue()

            print(f"Starting monitor at {now()}...")
            p = multiprocessing.Process(
                target=monitor_incoming_folder_loop,
                args=(str(incoming_dir), submit_q, 0.1, 1.5))
            p.start()
            time.sleep(0.1)
            assert submit_q.empty()

            print(f"Copying '{src}' to '{incoming_dir}' at {now()}...")
            dst_file = incoming_dir / src.name
            shutil.copy(src, dst_file)
            time.sleep(0.05)
            assert submit_q.empty()  # Should not be ready yet
            time.sleep(0.2)
            assert not submit_q.empty()  # Should be ready now
            assert str(Path(submit_q.get()).absolute()) == str(dst_file.absolute())
            assert submit_q.empty()  # Only one submission
            (incoming_dir / src.name).unlink()  # "Process" it by deleting

            print(f"Copying '{src_garbage}' to '{incoming_dir}' at {now()}...")
            shutil.copy(src_garbage, incoming_dir / src_garbage.name)
            time.sleep(0.3)
            assert not submit_q.empty()  # Should be submitted now
            submit_q.get()
            time.sleep(0.2)
            assert submit_q.empty()  # Must not be resubmitted yet
            time.sleep(0.2)
            assert submit_q.empty()
            time.sleep(2)
            assert not submit_q.empty()  # Should be resubmitted now

            print(f"Stopping monitor at {now()}...")
            p.terminate()



def _run_video_pipeline(db_file, dst_dir, mgt, stop_test: Callable):
    vpp = VideoProcessingPipeline(db_file, dst_dir, mgt, max_workers=1)
    p = Process(target=vpp.run_forever, args=[], kwargs={'poll_interval': 0.1})
    p.start()
    try:
        all_res = []
        for x in range(10):
            print(f"----- waiting user results round {x} -----")
            res = _wait_get(vpp.res_to_user, 1.0)
            if res:
                now = datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")
                print(f"-- GOT USER RESULT AT {now}:")
                pprint(res)
                all_res.append(res)
                if stop_test(res):
                    break
        return all_res
    finally:
        p.terminate()


@pytest.mark.timeout(120)
def test_ingest_errors(temp_dir):
    for err in ('bad_db', 'bad_rejects', 'bad_videos', 'bad_src'):

        for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
            with multiprocessing.Manager() as mgt:

                new_src = dst_dir/"incoming"/src.name
                shutil.copy(src, new_src)

                assert new_src.exists()

                md = video_metadata_reader.Results(
                    success = True,
                    src_file = '/dev/null' if err == 'bad_src' else new_src,
                    user_id = "testuser",
                    total_frames = 1234, duration = 1234/30.0,
                    orig_codec = "h264", fps = 30.0,
                    bitrate = 1*1024*1024,
                    metadata_all = {'meta': 'data'})

                vip = video_ingesting.VideoIngestingPool(
                    db_file = '/dev/null',
                    inq = mgt.Queue(), outq = mgt.Queue(), compress_q = mgt.Queue(),
                    videos_dir = '/dev/null' if err == 'bad_videos' else (dst_dir / "videos"),
                    reject_dir = '/dev/null' if err == 'bad_rejects' else (dst_dir / "rejected"),
                    max_workers=1)

                res = vip.on_recv_metadata(md = md, logging_name = "", test_mock = {})

                assert not res.success

                if err == 'bad_db':
                    assert 'db' in (res.msg + res.details).lower()
                    assert 'cleanup' not in (res.msg + res.details).lower()
                    assert list((dst_dir / 'videos').iterdir()) == []
                    assert not new_src.exists(), "Source was not moved to rejected/"
                    assert src.name in [x.name for x in (dst_dir / 'rejected').glob('**/*')]

                elif err in ('bad_rejects', 'bad_src'):
                    assert 'cleanup' in (res.msg + res.details).lower()
                                    


@pytest.mark.slow
@pytest.mark.timeout(120)
def test_video_pipeline_ok(temp_dir):
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        with multiprocessing.Manager() as mgt:
            shutil.copy(src, dst_dir/"incoming")

            all_res = _run_video_pipeline(db_file, dst_dir, mgt,
                lambda res: 'transcoded' in str(res))

            ok_res = [r for r in all_res if r.success]
            assert ok_res, "No successful results"
            assert not [r for r in all_res if not r.success], "Some failed results"

            vid = _get_video_from_db(ok_res[-1].video_hash, db_file)
            assert vid, "Video not found in DB"
            assert vid.orig_filename == src.name
            assert vid.added_by_userid == ok_res[-1].file_owner_id
            assert vid.recompression_done
            assert (dst_dir / 'videos' / ok_res[-1].video_hash / 'video.mp4').exists()
            assert (dst_dir / 'videos' / ok_res[-1].video_hash / 'orig' / Path(ok_res[-1].orig_file).name).exists()


@pytest.mark.timeout(120)
def test_video_pipeline_corrupted(temp_dir):
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        with multiprocessing.Manager() as mgt:
            shutil.copy(src_garbage, dst_dir/"incoming")
            all_res = _run_video_pipeline(db_file, dst_dir, mgt,
                lambda res: not res.success)
            assert [r for r in all_res if not r.success]


@pytest.mark.slow
@pytest.mark.timeout(120)
def test_video_pipeline_reupload_ok(temp_dir):
    for (src, dst_dir, src_garbage, src_lbr, db_file) in temp_dir:
        with multiprocessing.Manager() as mgt:

            shutil.copy(src_lbr, dst_dir/"incoming")
            all_res = _run_video_pipeline(db_file, dst_dir, mgt,
                lambda res: 'video added' in str(res).lower())

            ok_res = [r for r in all_res if r.success]
            assert ok_res
            assert not [r for r in all_res if not r.success]
            assert _get_video_from_db(ok_res[-1].video_hash, db_file)
            assert not (dst_dir / 'videos' / ok_res[-1].video_hash / 'video.mp4').exists()
            assert (dst_dir / 'videos' / ok_res[-1].video_hash / 'orig' / Path(ok_res[-1].orig_file).name).exists()

            # Reupload as same user
            shutil.copy(src_lbr, dst_dir/"incoming")
            all_res = _run_video_pipeline(db_file, dst_dir, mgt,
                lambda res: 'video added' in str(res).lower())

            ok_res = [r for r in all_res if r.success]
            assert ok_res
            assert not [r for r in all_res if not r.success]
            # Logs should contain not transcoding stuff
            assert 'already' in ' '.join([str(r).lower() for r in ok_res])
            assert 'transcod' not in ' '.join([str(r).lower() for r in ok_res])
            assert _get_video_from_db(ok_res[-1].video_hash, db_file)
            assert not (dst_dir / 'videos' / ok_res[-1].video_hash / 'video.mp4').exists()
            assert (dst_dir / 'videos' / ok_res[-1].video_hash / 'orig' / Path(ok_res[-1].orig_file).name).exists()
