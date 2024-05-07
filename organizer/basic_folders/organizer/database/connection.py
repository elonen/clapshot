import clapshot_grpc.clapshot.organizer as org
import sqlalchemy
from sqlalchemy.orm import sessionmaker

async def open_database(oi, server_info: org.ServerInfo):
    assert server_info.db.type == org.ServerInfoDatabaseDatabaseType.SQLITE, "Only SQLite is supported."

    oi.log.info(f"Opening SQLite database at '{server_info.db.endpoint}'")
    oi.db = sqlalchemy.create_engine(f"sqlite:///{server_info.db.endpoint}")
    oi.DbNewSession = sessionmaker(bind=oi.db)

    # For every connection, enable foreign keys. SQLite doesn't enforce them by default.
    sqlalchemy.event.listen(oi.db, 'connect', lambda c, _: c.execute('PRAGMA foreign_keys = ON;'))
