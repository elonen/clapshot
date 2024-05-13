use anyhow::Context;
use diesel::prelude::*;
use chrono::offset::Local;
use crate::database::{models, schema, DBResult, EmptyDBResult, to_db_res};

use super::{error::DBError, DbBasicQuery, PooledConnection};

// ------------------- Model-specific custom operations -------------------

impl models::User {
    pub fn set_name(conn: &mut PooledConnection, uid: &str, new_name: &str) -> EmptyDBResult
    {
        use schema::users::dsl::*;
        diesel::update(users.filter(id.eq(uid)))
            .set(name.eq(new_name))
            .execute(conn)?;
        Ok(())
    }

    /// Get a user by ID, or create a new user if it doesn't exist.
    ///
    /// # Arguments
    /// * `conn` - Database connection
    /// * `user_id` - ID of the user
    /// * `username` - Name of the user, if you want to update it. If None, and user is being created, the name will be set to the user_id.
    pub fn get_or_create(conn: &mut PooledConnection, user_id: &str, username: Option<&str>) -> DBResult<models::User>
    {
        match models::User::get(conn, &user_id.to_string()) {
            Ok(u) => {
                // Update name and admin status if needed
                if let Some(username) = username {
                    models::User::set_name(conn, &u.id, &username).context("Failed to update user name")?;
                }
                models::User::get(conn, &u.id)
            },
            Err(DBError::NotFound()) => {
                // User not found, create a new user
                let new_user = models::UserInsert {
                    id: user_id.to_string(),
                    name: username.unwrap_or(user_id).to_string(),
                };
                models::User::insert(conn, &new_user)
            },
            Err(e) => { Err(e) }
        }
    }
}


impl models::Video {

    /// Set the recompressed flag for a video.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the video
    pub fn set_recompressed(conn: &mut PooledConnection, vid: &str) -> EmptyDBResult
    {
        use schema::videos::dsl::*;
        diesel::update(videos.filter(id.eq(vid)))
            .set(recompression_done.eq(Local::now().naive_local()))
            .execute(conn)?;
        Ok(())
    }

    /// Set thumbnail sheet dimensions for a video.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `vid` - Id of the video
    /// * `cols` - Width of the thumbnail sheet
    /// * `rows` - Height of the thumbnail sheet
    pub fn set_thumb_sheet_dimensions(conn: &mut PooledConnection, vid: &str, cols: u32, rows: u32) -> EmptyDBResult
    {
        use schema::videos::dsl::*;
        diesel::update(videos.filter(id.eq(vid)))
            .set((thumb_sheet_cols.eq(cols as i32), thumb_sheet_rows.eq(rows as i32)))
            .execute(conn)?;
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
    pub fn rename(conn: &mut PooledConnection, vid: &str, new_name: &str) -> EmptyDBResult
    {
        use schema::videos::dsl::*;
        diesel::update(videos.filter(id.eq(vid)))
            .set(title.eq(new_name))
            .execute(conn)?;
        Ok(())
    }

    /// Get all videos that don't have thumbnails yet.
    ///
    /// # Returns
    /// * `Vec<models::Video>` - List of Video objects
    pub fn get_all_with_missing_thumbnails(conn: &mut PooledConnection) -> DBResult<Vec<models::Video>>
    {
        use models::*;
        use schema::videos::dsl::*;
        to_db_res(videos.filter(
                thumb_sheet_cols.is_null().or(
                thumb_sheet_rows.is_null()))
            .order_by(added_time.desc()).load::<Video>(conn))
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
    pub fn edit(conn: &mut PooledConnection, comment_id: i32, new_comment: &str) -> DBResult<bool>
    {
        use schema::comments::dsl::*;
        let res = diesel::update(comments.filter(id.eq(comment_id)))
            .set((comment.eq(new_comment), edited.eq(diesel::dsl::now))).execute(conn)?;
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
    pub fn set_seen(conn: &mut PooledConnection, msg_id: i32, new_status: bool) -> DBResult<bool>
    {
        use schema::messages::dsl::*;
        let res = diesel::update(messages.filter(id.eq(msg_id)))
            .set(seen.eq(new_status)).execute(conn)?;
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
    pub fn get_by_comment(conn: &mut PooledConnection, cid: i32) -> DBResult<Vec<models::Message>>
    {
        use schema::messages::dsl::*;
        to_db_res(messages.filter(comment_id.eq(cid)).load::<models::Message>(conn))
    }
}
