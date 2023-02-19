CREATE TABLE props (
       	id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
       	obj TEXT,
       	key TEXT NOT NULL,
       	val TEXT NOT NULL
);
CREATE INDEX ix_props_obj_key ON props (obj, key);
