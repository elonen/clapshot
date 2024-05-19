import re

def folder_path_to_uri_arg(folder_path: list[int]) -> str:
    """
    Convert a list of folder IDs to a URI string.
    """
    return ".".join(str(f) for f in folder_path)


def uri_arg_to_folder_path(uri: str|None) -> list[int]:
    """
    Convert a URI string to a list of folder IDs.
    """
    if not uri:
        return []
    if not re.match(r"^\d+(?:\.\d+)*$", uri):
        raise ValueError("Invalid folder path URI")
    return [int(f) for f in uri.split(".")]
