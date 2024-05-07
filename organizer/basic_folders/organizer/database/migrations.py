from dataclasses import dataclass
from textwrap import dedent

import clapshot_grpc.clapshot.organizer as org
import clapshot_grpc.clapshot as clap


# List of all specified migrations.
#
# Notes:
# - `uuid` is an arbitrary unique id for the migration
# - `version` is arbitrary, and _not_ unique, but must be sortable
#   - Multiple migrations with the same version make sense when they have different dependencies.
#     The engine will try to find a path to get all modules to their highest migration version,
#     by finding a path through the dependency graph.

@dataclass
class MigrationEntry:
    metadata: org.Migration
    up_sql: str


ALL_MIGRATIONS: list[MigrationEntry] = [

    MigrationEntry(
        metadata = org.Migration(
            uuid="basic_folders_2024-05-01_1610",
            version="0001_initial_schema",
            dependencies=[
                org.MigrationDependency(
                    name="clapshot.server",
                    min_ver="2023-04-18-190209_change_video_primkey",
                    max_ver=None
                ),
                org.MigrationDependency(
                    name="clapshot.organizer.basic_folders",
                    min_ver=None,
                    max_ver=""  # = database must have _no_ applied migrations yet for this module
                )
            ],
            description="Initial schema. One video per folder, folders are user-specific and can be nested."
        ),
        up_sql= dedent('''
                CREATE TABLE bf_folders (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
                    user_id VARCHAR(255) NOT NULL,
                    title VARCHAR(255) NOT NULL
                );

                CREATE INDEX bf_folders_user_id ON bf_folders(user_id);

                CREATE TABLE bf_folder_items (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,

                    folder_id INTEGER NOT NULL REFERENCES bf_folders(id) ON UPDATE CASCADE ON DELETE CASCADE,
                    sort_order INTEGER NOT NULL DEFAULT 0,

                    video_id VARCHAR(255) UNIQUE REFERENCES videos(id) ON UPDATE CASCADE ON DELETE CASCADE,
                    subfolder_id INTEGER UNIQUE REFERENCES bf_folders(id) ON UPDATE CASCADE ON DELETE CASCADE,

                    CHECK (
                        (video_id IS NOT NULL AND subfolder_id IS NULL) OR
                        (video_id IS NULL AND subfolder_id IS NOT NULL)
                    ),
                    CHECK (folder_id != subfolder_id)
                );

                CREATE INDEX bf_folder_items_folder_id ON bf_folder_items(folder_id);
                CREATE INDEX bf_folder_items_video_id ON bf_folder_items(video_id);
                CREATE INDEX bf_folder_items_subfolder_id ON bf_folder_items(subfolder_id);
                ''')
    ),

]