import json
from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus


def parse_json_args(args_str: str) -> dict:
    """
    Basically `json.loads`, but raises GRPCError on invalid input.
    """
    try:
        args = json.loads(args_str)
        if not isinstance(args, dict):
            raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Arguments must be a JSON object")
        return args
    except json.JSONDecodeError:
        raise GRPCError(GrpcStatus.INVALID_ARGUMENT, "Invalid JSON arguments")


def is_admin(user_id: str) -> bool:
    return user_id == "admin"       # If this was your custom organizer, you'd likeloy want to lookup LDAP or something here
