CREATE TABLE videos (
       	id INTEGER NOT NULL,
       	video_hash VARCHAR NOT NULL,
       	added_by_userid VARCHAR,
       	added_by_username VARCHAR,
       	added_time DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
       	recompression_done DATETIME,
       	orig_filename VARCHAR,
        title VARCHAR(255),
       	total_frames INTEGER,
       	duration FLOAT,
       	fps VARCHAR,
       	raw_metadata_all VARCHAR,
       	PRIMARY KEY (id)
);
CREATE INDEX ix_video_added_by_userid ON videos (added_by_userid);
CREATE UNIQUE INDEX ix_video_video_hash ON videos (video_hash);

CREATE TABLE comments (
       	id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       	video_hash VARCHAR NOT NULL,
       	parent_id INTEGER,
       	created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
       	edited DATETIME,
       	user_id VARCHAR NOT NULL,
       	username VARCHAR NOT NULL,
       	comment VARCHAR NOT NULL,
       	timecode VARCHAR,
       	drawing VARCHAR,
       	FOREIGN KEY(video_hash) REFERENCES videos (video_hash),
       	FOREIGN KEY(parent_id) REFERENCES comments (id)
);
CREATE INDEX ix_comment_parent_id ON comments (parent_id);

CREATE TABLE messages (
       	id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       	user_id VARCHAR NOT NULL,
       	created DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
       	seen BOOLEAN NOT NULL,
       	ref_video_hash VARCHAR,
       	ref_comment_id INTEGER,
       	event_name VARCHAR NOT NULL,
       	message VARCHAR NOT NULL,
       	details VARCHAR NOT NULL,
       	FOREIGN KEY(ref_comment_id) REFERENCES comments (id),
       	FOREIGN KEY(ref_video_hash) REFERENCES videos (video_hash)
);


------------------------------------------------------------------
-- A generic properties graph that Organizer plugins can use
-- as a way to store custom data about videos and comments
------------------------------------------------------------------

CREATE TABLE prop_nodes (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    node_type VARCHAR NOT NULL,
    body TEXT
);

CREATE TABLE prop_edges (
    from_video VARCHAR REFERENCES videos(video_hash) ON UPDATE CASCADE ON DELETE CASCADE,
    from_comment VARCHAR REFERENCES comments(id) ON UPDATE CASCADE ON DELETE CASCADE,
    from_node INT REFERENCES prop_nodes(id) ON UPDATE CASCADE ON DELETE CASCADE,

    to_video VARCHAR REFERENCES videos(video_hash) ON UPDATE CASCADE ON DELETE CASCADE,
    to_comment VARCHAR REFERENCES comments(id) ON UPDATE CASCADE ON DELETE CASCADE,
    to_node INT REFERENCES prop_nodes(id) ON UPDATE CASCADE ON DELETE CASCADE,

    edge_type VARCHAR NOT NULL,
    body TEXT,

    sort_order REAL NOT NULL DEFAULT 0.0,    -- For (GUI) sorting edges of the same type
    sibling_id INT NOT NULL DEFAULT 0,       -- For multiple edges of same type between same nodes

    -- Auto-replace duplicate edges. Use sibling_id to differentiate if needed.
    UNIQUE ( edge_type, sibling_id,
        from_video, from_comment, from_node,
        to_video, to_comment, to_node ) ON CONFLICT REPLACE
);

CREATE INDEX ix_prop_edges_from_video ON prop_edges (from_video, edge_type);
CREATE INDEX ix_prop_edges_from_comment ON prop_edges (from_comment, edge_type);
CREATE INDEX ix_prop_edges_from_node ON prop_edges (from_node, edge_type);
CREATE INDEX ix_prop_edges_to_video ON prop_edges (to_video, edge_type);
CREATE INDEX ix_prop_edges_to_comment ON prop_edges (to_comment, edge_type);
CREATE INDEX ix_prop_edges_to_node ON prop_edges (to_node, edge_type);

-- Ensure edge is connected from both ends
CREATE TRIGGER prop_edges_must_be_connected BEFORE INSERT ON prop_edges
    WHEN (NEW.from_video IS NULL AND NEW.from_comment IS NULL AND NEW.from_node IS NULL)
    OR   (NEW.to_video IS NULL AND NEW.to_comment IS NULL AND NEW.to_node IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'edges must be connected from both ends');
END;

-- Ensure only one flavor of from_* is set
CREATE TRIGGER prop_edges_from_at_most_one BEFORE INSERT ON prop_edges
    WHEN (NEW.from_video IS NOT NULL AND (NEW.from_comment IS NOT NULL OR NEW.from_node IS NOT NULL))
    OR   (NEW.from_comment IS NOT NULL AND NEW.from_node IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'one edge can only originate from one thing');
END;

-- Ensure only one flavor of to_* is set
CREATE TRIGGER prop_edges_to_at_most_one BEFORE INSERT ON prop_edges
    WHEN (NEW.to_video IS NOT NULL AND (NEW.to_comment IS NOT NULL OR NEW.to_node IS NOT NULL))
    OR   (NEW.to_comment IS NOT NULL AND NEW.to_node IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'each edge can only point to one thing');
END;

------ Helper views ------

-- All videos that point to a given node
CREATE VIEW videos_pointing_to_node AS
    SELECT
        to_node AS node_id, prop_nodes.node_type AS node_type, prop_nodes.body AS node_body,
        edge_type, body AS edge_body, sort_order, sibling_id,
        from_video AS video_hash, videos.title AS video_title, videos.duration AS video_duration
    FROM prop_edges
    LEFT JOIN prop_nodes ON prop_nodes.id = to_node
    LEFT JOIN videos ON videos.video_hash = from_video
    WHERE from_video IS NOT NULL;

-- (the other direction)
CREATE VIEW nodes_pointing_to_video AS
    SELECT
        to_video AS video_hash, videos.title AS video_title, videos.duration AS video_duration,
        edge_type, body AS edge_body, sort_order, sibling_id,
        from_node AS node_id, prop_nodes.node_type AS node_type, prop_nodes.body AS node_body
    FROM prop_edges
    LEFT JOIN prop_nodes ON prop_nodes.id = from_node
    LEFT JOIN videos ON videos.video_hash = to_video
    WHERE to_video IS NOT NULL;

-- All nodes that point to a given node
CREATE VIEW nodes_pointing_to_node AS
    SELECT
        to_node AS to_node_id, prop_nodes.node_type AS to_node_type, prop_nodes.body AS to_node_body,
        edge_type, body AS edge_body, sort_order AS edge_sort_order, sibling_id AS edge_sibling_id,
        from_node AS from_node_id, prop_nodes2.node_type AS from_node_type, prop_nodes2.body AS from_node_body
    FROM prop_edges
    LEFT JOIN prop_nodes ON prop_nodes.id = to_node
    LEFT JOIN prop_nodes AS prop_nodes2 ON prop_nodes2.id = from_node
    WHERE from_node IS NOT NULL;


CREATE VIEW nodes_without_outgoing_edges AS
    SELECT id, node_type, body FROM prop_nodes WHERE
        NOT EXISTS (SELECT 1 FROM prop_edges WHERE from_node = prop_nodes.id);

CREATE VIEW videos_without_outgoing_edges AS
    SELECT video_hash, title, duration FROM videos WHERE
        NOT EXISTS (SELECT 1 FROM prop_edges WHERE from_video = videos.video_hash);

CREATE VIEW node_count_outgoing_edges AS
    SELECT from_node AS node_id, n.body AS node_body, node_type, edge_type, COUNT(*) AS edge_count
        FROM prop_edges
        LEFT JOIN prop_nodes AS n ON n.id = from_node
        GROUP BY from_node, edge_type;

CREATE VIEW video_count_outgoing_edges AS
    SELECT from_video AS video_hash, v.title AS video_title, v.duration AS video_duration, edge_type, COUNT(*) AS edge_count
        FROM prop_edges
        LEFT JOIN videos AS v ON v.video_hash = from_video
        GROUP BY from_video, edge_type;

CREATE VIEW node_count_incoming_edges AS
    SELECT to_node AS node_id, n.body AS node_body, node_type, edge_type, COUNT(*) AS edge_count
        FROM prop_edges
        LEFT JOIN prop_nodes AS n ON n.id = to_node
        GROUP BY to_node, edge_type;

CREATE VIEW video_count_incoming_edges AS
    SELECT to_video AS video_hash, v.title AS video_title, v.duration AS video_duration, edge_type, COUNT(*) AS edge_count
        FROM prop_edges
        LEFT JOIN videos AS v ON v.video_hash = to_video
        GROUP BY to_video, edge_type;

------------------------------------------------------------------------
-- Implement a folder structure for videos using the prop_edges table
------------------------------------------------------------------------

CREATE PROCEDURE create_folder (IN label VARCHAR, IN parent INT NOT NULL, OUT new_folder_id ) AS
BEGIN
    SELECT RAISE(ABORT, 'parent must be a folder') FROM prop_nodes WHERE id = parent AND node_type != 'folder';
    INSERT INTO prop_nodes (node_type, body) VALUES ('folder', JSON_OBJECT('label', label));
    SET new_folder_id = last_insert_rowid();
    INSERT INTO prop_edges (from_node, to_node, edge_type) VALUES (new_folder_id, parent, 'parent_folder');
END;

CREATE PROCEDURE move_video_to_folder (IN video_hash VARCHAR(255), IN folder_id INT) AS
BEGIN
    SELECT RAISE(ABORT, 'to_node must be a folder') FROM prop_nodes WHERE id = folder_id AND node_type != 'folder';
    INSERT INTO prop_edges (from_video, to_node, edge_type) VALUES (video_hash, folder_id, 'parent_folder');
END;

CREATE PROCEDURE move_folder_to_folder (IN folder_id INT, IN parent_folder_id INT) AS
BEGIN
    SELECT RAISE(ABORT, 'from_node must be a folder') FROM prop_nodes WHERE id = folder_id AND node_type != 'folder';
    SELECT RAISE(ABORT, 'to_node must be a folder') FROM prop_nodes WHERE id = parent_folder_id AND node_type != 'folder';
    INSERT INTO prop_edges (from_node, to_node, edge_type) VALUES (folder_id, parent_folder_id, 'parent_folder');
END;

CREATE PROCEDURE get_folder_videos (IN folder_id INT) AS
BEGIN
    SELECT RAISE(ABORT, 'node must be a folder') FROM prop_nodes WHERE id = folder_id AND node_type != 'folder';
    SELECT * FROM videos_pointing_to_node WHERE node_id = folder_id AND edge_type = 'parent_folder';
END;

CREATE PROCEDURE get_subfolder (IN folder_id INT) AS
BEGIN
    SELECT RAISE(ABORT, 'node must be a folder') FROM prop_nodes WHERE id = folder_id AND node_type != 'folder';
    SELECT * FROM nodes_pointing_to_node WHERE from_node_id = folder_id AND edge_type = 'parent_folder';
END;

CREATE VIEW toplevel_folders AS
    SELECT * FROM prop_nodes WHERE node_type = 'folder' AND id NOT IN (
        SELECT to_node FROM prop_edges WHERE edge_type = 'parent_folder'
    );

------------------------------------------------------------------------

-- A "project" is just a top-level folder

CREATE PROCEDURE create_project (IN label VARCHAR, OUT new_folder_id ) AS
BEGIN
    INSERT INTO prop_nodes (node_type, body) VALUES ('folder', JSON_OBJECT('label', label));
    SET new_folder_id = last_insert_rowid();
END;

CREATE PROCEDURE add_project_member (IN project_id INT, IN username VARCHAR) AS
BEGIN
    SELECT RAISE(ABORT, 'project does not exist') FROM toplevel_folders WHERE id = project_id;
    -- Represent a project member as an edge from the project to itself,
    -- with the username as the edge body
    INSERT INTO prop_edges (from_node, to_node, edge_type, body) 
        VALUES (project_id, project_id, 'project_member', username);
END;

CREATE PROCEDURE remove_project_member (IN project_id INT, IN username VARCHAR) AS
BEGIN
    SELECT RAISE(ABORT, 'project does not exist') FROM toplevel_folders WHERE id = project_id;
    DELETE FROM prop_edges WHERE from_node = project_id AND to_node = project_id AND edge_type = 'project_member' AND body = username;
END;

-- Columns: project_id, project_label, username
CREATE VIEW project_members AS
    SELECT
        from_node AS project_id,
        JSON_EXTRACT(prop_nodes.body, '$.label') AS project_label,
        body AS username
    FROM prop_edges
    LEFT JOIN prop_nodes ON prop_nodes.id = from_node
    WHERE edge_type = 'project_member';


