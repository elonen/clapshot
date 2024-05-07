use diesel::prelude::*;
use chrono::offset::Local;
use crate::database::{models, schema, DB, DBResult, EmptyDBResult, to_db_res};

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
            .execute(&mut *db.conn()?.lock())?;
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
            .execute(&mut *db.conn()?.lock())?;
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
            .execute(&mut *db.conn()?.lock())?;
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
            .order_by(added_time.desc()).load::<Video>(&mut *db.conn()?.lock()))
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
            .set((comment.eq(new_comment), edited.eq(diesel::dsl::now))).execute(&mut *db.conn()?.lock())?;
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
            .set(seen.eq(new_status)).execute(&mut *db.conn()?.lock())?;
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
        to_db_res(messages.filter(comment_id.eq(cid)).load::<models::Message>(&mut *db.conn()?.lock()))
    }
}
