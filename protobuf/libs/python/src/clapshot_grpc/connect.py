from logging import Logger
from typing import Iterable
from grpclib import GRPCError
import grpclib
import grpclib.client
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.proto.clapshot.organizer as org

import sqlalchemy
from sqlalchemy.orm import sessionmaker


async def connect_back_to_server(
        server_info: org.ServerInfo,
        organizer_name: str,
        organizer_version: Iterable,   # (major, minor, patch)
        organizer_description: str,
        hard_dependencies: list[org.OrganizerDependency],
        log: Logger) -> org.OrganizerOutboundStub:
    """
    Connect Organizer back to the Clapshot server, using the TCP or Unix socket
    address provided in the handshake.
    """
    try:
        try:
            organizer_version = tuple(int(x) for x in organizer_version)
            assert len(organizer_version) == 3
        except ValueError:
            raise ValueError("organizer_version must be a tuple of 3 int-castable numbers (major, minor, patch)")

        if tcp := server_info.backchannel.tcp:
            backchannel = grpclib.client.Channel(host=tcp.host, port=tcp.port)
        else:
            backchannel = grpclib.client.Channel(path=server_info.backchannel.unix.path)

        log.info("Connecting back to Clapshot server...")
        oos = org.OrganizerOutboundStub(backchannel)
        await oos.handshake(org.OrganizerInfo(
            version=org.SemanticVersionNumber(major=organizer_version[0], minor=organizer_version[1], patch=organizer_version[2]),
            name=organizer_name,
            description=organizer_description,
            hard_dependencies=hard_dependencies,
        ))
        log.info("Clapshot server connected.")
        return oos

    except ConnectionRefusedError as e:
        log.error(f"Return connection to Clapshot server refused: {e}")
        raise GRPCError(GrpcStatus.UNKNOWN, "Failed to connect back to you (the Clapshot server)")



async def open_database(server_info: org.ServerInfo, debug_sql: bool, log: Logger) -> tuple[sqlalchemy.Engine, sqlalchemy.orm.sessionmaker]:
    """
    Open the SQLite database specified in the server info, make a new session factory and return both.
    If debug_sql is True, SQLalchemy will log queries.
    All connections will have necessary pragmas applied (foreign keys, WAL, etc.)
    """
    assert server_info.db.type == org.ServerInfoDatabaseDatabaseType.SQLITE, "Only SQLite is supported."

    log.info(f"Opening SQLite database at '{server_info.db.endpoint}'")
    db = sqlalchemy.create_engine(f"sqlite:///{server_info.db.endpoint}", connect_args={'timeout': 15}, echo=debug_sql)
    db_new_session = sessionmaker(bind=db)

    # For every connection, enable foreign keys. SQLite doesn't enforce them by default.
    def apply_pragmas(connection, _):
        for pragma in '''
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA wal_autocheckpoint = 1000;
            PRAGMA wal_checkpoint(TRUNCATE);
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 15000;
        '''.split(';'):
            connection.execute(pragma)

    sqlalchemy.event.listen(db, 'connect', apply_pragmas)
    return db, db_new_session
