from __future__ import annotations

import clapshot_grpc.clapshot.organizer as org
import sqlalchemy
from sqlalchemy.orm import sessionmaker

import organizer

async def open_database(oi: organizer.OrganizerInbound, server_info: org.ServerInfo):
    assert server_info.db.type == org.ServerInfoDatabaseDatabaseType.SQLITE, "Only SQLite is supported."

    oi.log.info(f"Opening SQLite database at '{server_info.db.endpoint}'")
    oi.db = sqlalchemy.create_engine(f"sqlite:///{server_info.db.endpoint}", connect_args={'timeout': 15})  #, echo=oi.debug)
    oi.db_new_session = sessionmaker(bind=oi.db)

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
    sqlalchemy.event.listen(oi.db, 'connect', apply_pragmas)
