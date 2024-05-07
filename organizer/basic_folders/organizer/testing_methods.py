from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus
import clapshot_grpc.clapshot.organizer as org


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
