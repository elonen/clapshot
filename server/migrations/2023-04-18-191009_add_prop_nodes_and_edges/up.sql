------------------------------------------------------------------
-- A generic properties graph that Organizer plugins can use
-- as a way to store custom data about videos and comments
------------------------------------------------------------------

CREATE TABLE prop_nodes (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    node_type VARCHAR NOT NULL,
    body TEXT,

    -- If the node should be a singleton, set this key and node_type to identify it.
    singleton_key VARCHAR DEFAULT NULL,
    UNIQUE (node_type, singleton_key) ON CONFLICT REPLACE
);

CREATE TABLE prop_edges (
    id INTEGER NOT NULL PRIMARY KEY,

    from_video VARCHAR REFERENCES videos(id) ON UPDATE CASCADE ON DELETE CASCADE,
    from_comment INT REFERENCES comments(id) ON UPDATE CASCADE ON DELETE CASCADE,
    from_node INT REFERENCES prop_nodes(id) ON UPDATE CASCADE ON DELETE CASCADE,

    to_video VARCHAR REFERENCES videos(id) ON UPDATE CASCADE ON DELETE CASCADE,
    to_comment INT REFERENCES comments(id) ON UPDATE CASCADE ON DELETE CASCADE,
    to_node INT REFERENCES prop_nodes(id) ON UPDATE CASCADE ON DELETE CASCADE,

    edge_type VARCHAR NOT NULL,
    body TEXT,

    sort_order REAL NOT NULL DEFAULT 0.0,    -- For (GUI) sorting edges of the same type, larger is later
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
