import multiprocessing
from pathlib import Path
import random
import sys
import time
import socketio

from pytest_cov.embed import cleanup_on_sigterm
cleanup_on_sigterm()

import pytest
from clapshot_server import main

random.seed()

@pytest.mark.slow
@pytest.mark.timeout(15)
def test_main_starts(tmp_path_factory):
    """
    Test that the main function starts the server.
    """
    dst_dir = Path(tmp_path_factory.mktemp("clapshot_test"))

    old_argv = sys.argv
    p = multiprocessing.Process(target=main.main)

    try:
        port = random.randint(10000, 20000)
        sys.argv = ['clapshot-server', 
                '--url-base', f'http://127.0.0.1:{port}',
                '--port', str(port),
                '--host', '127.0.0.1',
                '--data-dir', str(Path(dst_dir).absolute()),
                '--host-videos',
                '--poll', '0.1',
                '--debug']
        p.start()        
        time.sleep(1)

        # Cause some action
        with open(dst_dir / 'incoming' / 'garbage.mov', 'w') as f:
            f.write('test')

        time.sleep(2)
        assert p.is_alive()

        # Test API connection
        sio = socketio.Client()
        sio.connect(
            url = f'http://127.0.0.1:{port}', 
            socketio_path = '/api/socket.io',
            headers={'X-REMOTE-USER-ID': 'user1', 'X-REMOTE-USER-NAME': 'user1.name'})
        assert sio.connected

        sio.emit('list_my_videos', {})
        time.sleep(1)
        assert p.is_alive()

        sio.disconnect()

    finally:
        sys.argv = old_argv
        p.kill()

    assert (dst_dir / "clapshot.sqlite").exists()
