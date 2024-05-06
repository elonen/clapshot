import asyncio
import sys
import re

from docopt import docopt
import grpclib.server

from config import VERSION
from organizer.helpers.logger import make_logger
from organizer import OrganizerInbound


async def main():
    doc = """
    Clapshot Organizer plugin that implements basic folders for the UI.

    This gRPC server can bind to Unix socket or TCP address, and is typically
    executed by the Clapshot server as a subprocess, in which case the server also
    provides the `bind` argument. You can, however, also run it manually and
    configure the server to connect to it via TCP.

    Usage:
      {NAME} [options] <bind>
      {NAME} (-h | --help)
      {NAME} (-v | --version)

    Required:
        <bind>              Unix socket or IP address to bind to.
                            e.g. '/tmp/organizer.sock', '[::1]:50051', '127.0.0.1:50051'

    Options:
     -d --debug             Enable debug logging
     -j --json              Log in JSON format
     -t --tcp               Use TCP instead of Unix socket
     -h --help              Show this screen
     -v --version           Show version
    """
    global debug_logging, json_logging
    arguments = docopt(doc.format(NAME=sys.argv[0]), version=VERSION)
    flag_debug, flag_json, flag_tcp = arguments["--debug"], arguments["--json"], arguments["--tcp"]

    bind_addr = arguments["<bind>"]
    logger=make_logger("bf", debug=flag_debug, json=flag_json)

    server = grpclib.server.Server([OrganizerInbound(logger, flag_debug)])
    if flag_tcp:
        assert re.match(r"^\d+\.\d+\.\d+\.\d+:\d+$", bind_addr) or re.match(r"^\[[0-9a-f:]*\]:\d+$", bind_addr), \
            "bind_addr must be in the format of 'ip_listen_address:port' when using TCP"
        host, port = bind_addr.rsplit(":", 1)
        await server.start(host=host, port=int(port))
    else:
        await server.start(path=bind_addr)  # unix socket
    logger.info(f"Organizer listening on '{bind_addr}'")

    await server.wait_closed()
    logger.info("Organizer stopped listening.")


if __name__ == '__main__':
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("EXIT signaled.")
