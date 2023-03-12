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

------------------------------------------------------------------
------ Enforce some constraints on the graph
------------------------------------------------------------------

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

------------------------------------------------------------------
------ Helper views for querying the graph
------------------------------------------------------------------

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


-- Fast listing of "root" nodes

CREATE VIEW nodes_without_outgoing_edges AS
    SELECT id, node_type, body FROM prop_nodes WHERE
        NOT EXISTS (SELECT 1 FROM prop_edges WHERE from_node = prop_nodes.id);

CREATE VIEW videos_without_outgoing_edges AS
    SELECT video_hash, title, duration FROM videos WHERE
        NOT EXISTS (SELECT 1 FROM prop_edges WHERE from_video = videos.video_hash);


-- Counting per edge type and direction

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

