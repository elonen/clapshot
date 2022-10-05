import asyncio
import json
import multiprocessing
from pathlib import Path
import random
import sys
import time
import aiohttp

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
        async def test_ws():
            async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=10)) as session:
                async with session.ws_connect(
                    f'http://127.0.0.1:{port}/api/ws',
                    headers={'HTTP_X_REMOTE_USER_ID': 'user1', 'HTTP_X_REMOTE_USER_NAME': 'user1.name'}) as ws:
                        await ws.send_str('{"cmd": "list_my_videos"}')
                        msg = await ws.receive(timeout=5)
                        assert msg.type == aiohttp.WSMsgType.TEXT
                        msg = json.loads(msg.data)
                        assert msg['cmd']
                        assert msg['data']

        asyncio.run(test_ws())

    finally:
        sys.argv = old_argv
        p.kill()

    assert (dst_dir / "clapshot.sqlite").exists()
