import json
import re
from typing import Optional
from logging import Logger

import grpclib
from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus
from grpclib.server import Server

import clapshot_grpc.clapshot as clap
import clapshot_grpc.clapshot.organizer as org

import sqlalchemy
from sqlalchemy.orm import sessionmaker, Session

from config import VERSION, MODULE_NAME, PATH_COOKIE_NAME

from .database.operations import db_check_and_fix_integrity, db_check_for_folder_loops, db_check_pending_migrations, db_apply_migration, db_test_orm_mappings
from .database.models import DbFolder, DbFolderItems, DbVideo


async def check_migrations(oi, req: org.CheckMigrationsRequest) -> org.CheckMigrationsResponse:
    """
    Organizer method (gRPC/protobuf)

    Called when during server startup to check if there are any pending DB migrations for this module.
      => Checks the db for the current schema version and returns a list migrations that have a higher version string.
         Server will figure out which one(s) to apply based on dependencies, and then call apply_migration().
    """
    cur_ver, pending = db_check_pending_migrations(oi.db)
    max_ver = sorted([m.version for m in pending], reverse=True)[0] if pending else cur_ver
    oi.log.info(f"check_migrations(): current schema version = '{cur_ver}', max version = '{max_ver}', {len(pending)} pending migration alternatives")
    return org.CheckMigrationsResponse(current_schema_ver=cur_ver,pending_migrations=pending)


async def apply_migration(oi, req: org.ApplyMigrationRequest) -> org.ApplyMigrationResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server to apply a single pending migration to the database. The migration is identified by its UUID,
    previously returned by check_migrations().
    """
    oi.log.info(f"apply_migration('{req.uuid}')")
    with oi.DbNewSession() as dbs:
        db_apply_migration(dbs, req.uuid)
        return org.ApplyMigrationResponse()


async def after_migrations(oi, _: org.AfterMigrationsRequest) -> clap.Empty:
    """
    Organizer method (gRPC/protobuf)

    Called by the server after all pending migrations have been applied, to perform any necessary startup initialization.
      => Do some "fsck"-type operations on the database.
    """
    log = oi.log.getChild("after_migration")
    with oi.DbNewSession() as dbs:
        db_test_orm_mappings(dbs, log)
        db_check_and_fix_integrity(dbs, log)
        db_check_for_folder_loops(dbs, log)
        return clap.Empty()
