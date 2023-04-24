use diesel::prelude::*;
use chrono::offset::Local;
use crate::database::{models, schema, DB, DBError, DBResult, DBPaging, EmptyDBResult, to_db_res};
use super::GraphObjId;

// ------------------- Model-specific custom operations -------------------


impl models::Video {

    /// Set the recompressed flag for a video.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the video
    pub fn set_recompressed(db: &DB, vid: &str) -> EmptyDBResult
    {
        use schema::videos::dsl::*;
        diesel::update(videos.filter(id.eq(vid)))
            .set(recompression_done.eq(Local::now().naive_local()))
            .execute(&mut db.conn()?)?;
        Ok(())
    }

    /// Set thumbnail sheet dimensions for a video.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the video
    /// * `cols` - Width of the thumbnail sheet
    /// * `rows` - Height of the thumbnail sheet
    pub fn set_thumb_sheet_dimensions(db: &DB, vid: &str, cols: u32, rows: u32) -> EmptyDBResult
    {
        use schema::videos::dsl::*;
        diesel::update(videos.filter(id.eq(vid)))
            .set((thumb_sheet_cols.eq(cols as i32), thumb_sheet_rows.eq(rows as i32)))
            .execute(&mut db.conn()?)?;
        Ok(())
    }

    /// Rename a video (title).
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the video
    /// * `new_name` - New title
    ///
    /// # Returns
    /// * `EmptyResult`
    /// * `Err(NotFound)` - Video not found
    /// * `Err(Other)` - Other error
    pub fn rename(db: &DB, vid: &str, new_name: &str) -> EmptyDBResult
    {
        use schema::videos::dsl::*;
        diesel::update(videos.filter(id.eq(vid)))
            .set(title.eq(new_name))
            .execute(&mut db.conn()?)?;
        Ok(())
    }

    /// Get all videos that don't have thumbnails yet.
    ///
    /// # Returns
    /// * `Vec<models::Video>` - List of Video objects
    pub fn get_all_with_missing_thumbnails(db: &DB) -> DBResult<Vec<models::Video>>
    {
        use models::*;
        use schema::videos::dsl::*;
        to_db_res(videos.filter(
                thumb_sheet_cols.is_null().or(
                thumb_sheet_rows.is_null()))
            .order_by(added_time.desc()).load::<Video>(&mut db.conn()?))
    }
}


impl models::Comment {

    /// Edit a comment (change text).
    ///
    /// # Arguments
    /// * `comment_id` - ID of the comment
    /// * `new_comment` - New text of the comment
    ///
    /// # Returns
    /// * `Res<bool>` - True if comment was edited, false if it was not found
    pub fn edit(db: &DB, comment_id: i32, new_comment: &str) -> DBResult<bool>
    {
        use schema::comments::dsl::*;
        let res = diesel::update(comments.filter(id.eq(comment_id)))
            .set((comment.eq(new_comment), edited.eq(diesel::dsl::now))).execute(&mut db.conn()?)?;
        Ok(res > 0)
    }
}


impl models::Message {

    /// Set the seen status of a message.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `msg_id` - ID of the message
    /// * `new_status` - New status
    ///
    /// # Returns
    /// * `Res<bool>` - True if message was found and updated, false if it was not found
    pub fn set_seen(db: &DB, msg_id: i32, new_status: bool) -> DBResult<bool>
    {
        use schema::messages::dsl::*;
        let res = diesel::update(messages.filter(id.eq(msg_id)))
            .set(seen.eq(new_status)).execute(&mut db.conn()?)?;
        Ok(res > 0)
    }

    /// Get all messages for a given comment.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `cid` - ID of the comment
    ///
    /// # Returns
    /// * `Res<Vec<models::Message>>` - List of messages
    pub fn get_by_comment(db: &DB, cid: i32) -> DBResult<Vec<models::Message>>
    {
        use schema::messages::dsl::*;
        to_db_res(messages.filter(comment_id.eq(cid)).load::<models::Message>(&mut db.conn()?))
    }
}


impl models::PropNode {

    /// Get (some) nodes from the prop graph database, filtered by node type.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `node_type` - Node type to filter by.
    /// * `node_ids` - Optional list of node IDs to filter by. If None, consider all nodes.
    pub fn get_by_type(db: &DB, node_type: &str, node_ids: &Option<Vec<i32>>) -> DBResult<Vec<models::PropNode>>
    {
        let xnt = node_type;
        {
            use schema::prop_nodes::dsl::*;
            let q = prop_nodes.filter(node_type.eq(xnt)).into_boxed();
            let q = if let Some(node_ids) = node_ids { q.filter(id.eq_any(node_ids)) } else { q };
            to_db_res(q.load::<models::PropNode>(&mut db.conn()?))
        }
    }

}


impl models::PropEdge {

    // Query edges, filtered by edge type, IDs and/or from/to node.
    // Each filter is optional, so all None will return all edges in the database.
    pub fn get_filtered(db: &DB, from_id: Option<GraphObjId>, to_id: Option<GraphObjId>, edge_type: Option<&str>, edge_ids: &Option<Vec<i32>>, pg: DBPaging)
        -> DBResult<Vec<models::PropEdge>>
    {
        let et = edge_type; {
            use schema::prop_edges::dsl::*;
            let q = prop_edges.into_boxed();
            let q = if let Some(edge_ids) = edge_ids { q.filter(id.eq_any(edge_ids)) } else { q };
            let q = if let Some(et) = et { q.filter(edge_type.eq(et)) } else { q };
            let q = match from_id {
                Some(GraphObjId::Video(vid)) => q.filter(from_video.eq(vid)),
                Some(GraphObjId::Comment(cid)) => q.filter(from_comment.eq(cid)),
                Some(GraphObjId::Node(nid)) => q.filter(from_node.eq(nid)),
                None => q
            };
            let q = match to_id {
                Some(GraphObjId::Video(vid)) => q.filter(to_video.eq(vid)),
                Some(GraphObjId::Comment(cid)) => q.filter(to_comment.eq(cid)),
                Some(GraphObjId::Node(nid)) => q.filter(to_node.eq(nid)),
                None => q
            };
            to_db_res(q.order(sort_order.asc())
                .then_order_by(id.asc())
                .offset(pg.offset())
                .limit(pg.limit())
                .load::<models::PropEdge>(&mut db.conn()?))
        }
    }

    // Convert the database from_video/comment/node fields into a GraphObjId
    pub fn obj_id_from(&self) -> DBResult<GraphObjId>
    {
        match (&self.from_video, self.from_comment, self.from_node) {
            (Some(vid), None, None) => Ok(GraphObjId::Video(&vid)),
            (None, Some(cid), None) => Ok(GraphObjId::Comment(cid)),
            (None, None, Some(nid)) => Ok(GraphObjId::Node(nid)),
            _ => Err(DBError::NotFound())
        }
    }

    // Convert the database to_video/comment/node fields into a GraphObjId
    pub fn obj_id_to(&self) -> DBResult<GraphObjId>
    {
        match (&self.to_video, self.to_comment, self.to_node) {
            (Some(vid), None, None) => Ok(GraphObjId::Video(&vid)),
            (None, Some(cid), None) => Ok(GraphObjId::Comment(cid)),
            (None, None, Some(nid)) => Ok(GraphObjId::Node(nid)),
            _ => Err(DBError::NotFound())
        }
    }
}
