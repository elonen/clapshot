import os

VERSION = "0.9.2"
MODULE_NAME = "clapshot.organizer.basic_folders"
PATH_COOKIE_NAME = "folder_path"

# Metaplugin configuration
# All .py files in this directory will be loaded as plugins
METAPLUGINS_DIR = os.environ.get(
    "CLAPSHOT_METAPLUGINS_DIR",
    "/opt/clapshot-org-bf-metaplugins"
)
