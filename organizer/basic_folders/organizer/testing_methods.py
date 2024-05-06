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

from .database.operations import db_check_and_fix_integrity, db_check_pending_migrations, db_apply_migration, db_test_orm_mappings
from .database.models import DbFolder, DbFolderItems, DbVideo


async def list_tests(oi) -> org.ListTestsResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server to list all available unit/integrations tests in this plugin.
    """
    oi.log.info("list_tests")
    return org.ListTestsResponse(test_names=[])

async def run_test(oi, _: org.RunTestRequest) -> org.RunTestResponse:
    """
    Organizer method (gRPC/protobuf)

    Called by the server to run a single unit/integration test in this plugin.
    """
    oi.log.info("run_test")
    raise GRPCError(GrpcStatus.UNIMPLEMENTED)
