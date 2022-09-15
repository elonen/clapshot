video_metadata_sql = """
    CREATE TABLE metadata (
        video_id TEXT PRIMARY KEY,
        added_by TEXT,
        added_time DATETIME,
        orig_filename TEXT,
        orig_codec TEXT,
        n_frames INTEGER,
        fps NUMERIC,
        raw_metadata TEXT
    );
    """

comment_table_sql = """
    CREATE TABLE comments (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        parent_id INTEGER DEFAULT NULL,
        timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
        username TEXT,
        comment TEXT,
        avatar_url TEXT DEFAULT NULL,
        drawing BLOB DEFAULT NULL,
        FOREIGN KEY (parent_id) REFERENCES comments (id)
    );
    """

DATA_DIR = "test_data"
INCOMING_DIR = f"{DATA_DIR}/incoming"
REJECTED_DIR = f"{DATA_DIR}/rejected"
VIDEOS_DIR = f"{DATA_DIR}/videos"
DB_FILE = f"{DATA_DIR}/clapshot.db"
