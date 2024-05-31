from __future__ import annotations
from logging import Logger

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org
from clapshot_grpc.connect import open_database
from organizer.config import MODULE_NAME

from .database.operations import db_check_and_fix_integrity, db_check_for_folder_loops, db_check_pending_migrations, db_apply_migration, db_test_orm_mappings
import organizer


async def check_migrations_impl(req: org.CheckMigrationsRequest, log: Logger) -> org.CheckMigrationsResponse:
    """
    Organizer method (gRPC/protobuf)

    Called when during server startup to check if there are any pending DB migrations for this module.
      => Checks the db for the current schema version and returns a list migrations that have a higher version string.
         Server will figure out which one(s) to apply based on dependencies, and then call apply_migration().
    """
    db, _ = await open_database(req.db, False, log)
    cur_ver, pending = db_check_pending_migrations(db)
    max_ver = sorted([m.version for m in pending], reverse=True)[0] if pending else cur_ver
    return org.CheckMigrationsResponse(
        name=MODULE_NAME,
        current_schema_ver=cur_ver,
        pending_migrations=pending)


async def apply_migration_impl(req: org.ApplyMigrationRequest, log: Logger) -> org.ApplyMigrationResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server to apply a single pending migration to the database. The migration is identified by its UUID,
    previously returned by check_migrations().
    """
    _db, session_maker = await open_database(req.db, False, log)
    with session_maker() as dbs:
        db_apply_migration(dbs, req.uuid, log)
        return org.ApplyMigrationResponse()


async def db_integrity_tests(oi: organizer.OrganizerInbound):
    """
    Do some "fsck"-type operations on the database.
    """
    log = oi.log.getChild("after_migration")
    log.debug("Running post-migration checks...")
    with oi.db_new_session() as dbs:
        db_test_orm_mappings(dbs, log)
        db_check_for_folder_loops(dbs, log)
        db_check_and_fix_integrity(dbs, log)
