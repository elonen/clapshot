import json
import re
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


def folder_path_to_uri_arg(folder_path: list[int]) -> str:
    """
    Convert a list of folder IDs to a URI string.
    """
    return "-".join(str(f) for f in folder_path)


def uri_arg_to_folder_path(uri: str|None) -> list[int]:
    """
    Convert a URI string to a list of folder IDs.
    """
    if not uri:
        return []
    if not re.match(r"^\d+(?:-\d+)*$", uri):
        raise ValueError("Invalid folder path URI")
    return [int(f) for f in uri.split("-")]
