import json

from grpclib.exceptions import GRPCError
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org


async def try_send_user_message(srv: org.OrganizerOutboundStub, msg_req: org.ClientShowUserMessageRequest) -> GRPCError | None:
    """
    Try to send a user message to the client, return GRPCError exceptions instead of raising them.
    Other exceptions are raised as usual.
    """
    try:
        await srv.client_show_user_message(msg_req)
        return None
    except GRPCError as e:
        return e


def parse_json_dict(args_str: str) -> dict:
    """
    Calls `json.loads`, but raises GRPCError if the result is not a dict.
    """
    try:
        args = json.loads(args_str)
        if not isinstance(args, dict):
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Arguments must be a JSON object")
        return args
    except json.JSONDecodeError:
        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Invalid JSON arguments")
