use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager};
use diesel::SqliteConnection;

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use std::path::{Path};

use chrono::offset::Local;

pub mod schema;
pub mod models;
pub mod error;

use error::{DBError, DBResult, EmptyDBResult};

pub type Pool = diesel::r2d2::Pool<ConnectionManager<SqliteConnection>>;
type PooledConnection = r2d2::PooledConnection<ConnectionManager<SqliteConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();


/// Convert a diesel result to a DBResult, turning empty result
/// into a DBError::NotFound
fn to_db_res<U>(res: QueryResult<U>) -> DBResult<U> {
    let res = res.optional();
    match res {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(DBError::NotFound()),
        Err(e) => Err(DBError::BackendError(e)),
    }
}


pub struct DB {
    pool: Pool,
}

impl DB {

    /// Connect to SQLite database with an URL (use this for memory databases)
    pub fn connect_db_url( db_url: &str ) -> DBResult<DB> {
        let manager = ConnectionManager::<SqliteConnection>::new(db_url);
        let pool = Pool::builder().build(manager)
            .map_err(|e| DBError::Other(e.to_string()))?;
        Ok(DB { pool: pool })
    }

    /// Connect to SQLite database with a file path
    pub fn connect_db_file( db_file: &Path ) -> DBResult<DB> {
        let db_url = format!("sqlite://{}", db_file.to_str().ok_or("Invalid DB file path")
            .map_err(|e| DBError::Other(e.to_string()))?);
        DB::connect_db_url(&db_url)
    }


    /// Get a connection from the pool
    pub fn conn(&self) ->  DBResult<PooledConnection> {
        self.pool.get().map_err(|e| DBError::Other(e.to_string()))
    }

    // Check if database is up-to-date compared to the embedded migrations
    pub fn migrations_needed(&self) -> DBResult<bool> {
        let mut conn = self.conn()?;
        MigrationHarness::has_pending_migration(&mut conn, MIGRATIONS)
            .map_err(|e| DBError::Other(e.to_string()))
    }

    /// Run DB migrations (or create DB if empty)
    pub fn run_migrations(&self) -> EmptyDBResult
    {
        let mut conn = self.conn()?;
        conn.run_pending_migrations(MIGRATIONS).map_err(|e| DBError::Other(e.to_string()))?;
        Ok(())
    }

    // -----------------------------------------------------------------------------------------------

    /// Add a new video to the database.
    /// 
    /// # Arguments
    /// * `video` - Video object
    /// 
    /// # Returns
    /// * `sql.Integer` - ID of the new video
    pub fn add_video(&self, video: &models::VideoInsert) -> DBResult<i32>
    {
        use schema::videos::dsl::*;
        let res = diesel::insert_into(videos)
            .values(video).returning(id).get_result(&mut self.conn()?)?;
        Ok(res)
    }

    /// Set the recompressed flag for a video.
    /// 
    /// # Arguments
    /// * `vh` - Hash (unique identifier) of the video
    pub fn set_video_recompressed(&self, vh: &str) -> EmptyDBResult
    {
        use schema::videos::dsl::*;
        diesel::update(videos.filter(video_hash.eq(vh)))
            .set(recompression_done.eq(Local::now().naive_local()))
            .execute(&mut self.conn()?)?;
        Ok(())
    }

    /// Get a video from the database.
    /// 
    /// # Arguments
    /// * `vh` - Hash (unique identifier) of the video
    ///
    /// # Returns
    /// * `models::Video` - Video object
    /// * `Err(NotFound)` - Video not found
    pub fn get_video(&self, vh: &str) -> DBResult<models::Video>
    {
        use models::*;
        use schema::videos::dsl::*;
        to_db_res(videos.filter(video_hash.eq(vh)).first::<Video>(&mut self.conn()?))
    }

    /// Delete a video and all its comments from the database.
    ///     
    /// # Arguments
    /// * `vh` - Hash (unique identifier) of the video
    /// 
    /// # Returns
    /// * `EmptyResult`
    /// * `Err(NotFound)` - Video not found
    pub fn del_video_and_comments(&self, vh: &str) -> EmptyDBResult
    {
        use schema::videos::dsl as sv;
        use schema::comments::dsl as sc;
        let conn = &mut self.conn()?;
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            diesel::delete(sv::videos.filter(sv::video_hash.eq(vh))).execute(conn)?;
            diesel::delete(sc::comments.filter(sc::video_hash.eq(vh))).execute(conn)?;
            Ok(())
        })?;
        Ok(())
    }

    /// Get all videos for a user.
    /// 
    /// # Arguments
    /// * `user_id` - User ID
    /// 
    /// # Returns
    /// * `Vec<models::Video>` - List of Video objects
    pub fn get_all_user_videos(&self, user_id: &str) -> DBResult<Vec<models::Video>>
    {
        use models::*;
        use schema::videos::dsl::*;
        to_db_res(videos.filter(added_by_userid.eq(user_id)).load::<Video>(&mut self.conn()?))
    }

    /// Add a new comment on a video.
    /// 
    /// # Arguments
    /// * `comment` - Comment object
    /// 
    /// # Returns
    /// * `i32` - ID of the new comment
    pub fn add_comment(&self, cmt: &models::CommentInsert) -> DBResult<i32>
    {
        use schema::comments::dsl::*;
        let res = diesel::insert_into(comments)
            .values(cmt).returning(id).get_result(&mut self.conn()?)?;
        Ok(res)
    }

    /// Get a comment from the database.
    /// 
    /// # Arguments
    /// * `comment_id` - ID of the comment
    /// 
    /// # Returns
    /// * `models::Comment` - Comment object
    /// * `Err(NotFound)` - Comment not found
    pub fn get_comment(&self, comment_id: i32 ) -> DBResult<models::Comment>
    {
        use models::*;
        use schema::comments::dsl::*;
        to_db_res(comments.filter(id.eq(comment_id)).first::<Comment>(&mut self.conn()?))
    }

    /// Get all comments for a video.
    /// 
    /// # Arguments
    /// * `vh` - Hash (unique identifier) of the video
    /// 
    /// # Returns
    /// * `Vec<models::Comment>` - List of Comment objects
    pub fn get_video_comments(&self, vh: &str ) -> DBResult<Vec<models::Comment>>
    {
        use models::*;
        use schema::comments::dsl::*;
        Ok(comments.filter(video_hash.eq(vh)).load::<Comment>(&mut self.conn()?)?)
    }

    /// Delete a comment from the database.
    /// 
    /// # Arguments
    /// * `comment_id` - ID of the comment
    /// 
    /// # Returns
    /// * `Res<bool>` - True if comment was deleted, false if it was not found
    pub fn del_comment(&self, comment_id: i32 ) -> DBResult<bool>
    {
        use schema::comments::dsl::*;
        let res = diesel::delete(comments.filter(id.eq(comment_id))).execute(&mut self.conn()?)?;
        Ok(res > 0)
    }

    /// Edit a comment (change text).
    /// 
    /// # Arguments
    /// * `comment_id` - ID of the comment
    /// * `new_comment` - New text of the comment
    /// 
    /// # Returns
    /// * `Res<bool>` - True if comment was edited, false if it was not found
    pub fn edit_comment(&self, comment_id: i32, new_comment: &str) -> DBResult<bool>
    {
        use schema::comments::dsl::*;
        let res = diesel::update(comments.filter(id.eq(comment_id)))
            .set((comment.eq(new_comment), edited.eq(diesel::dsl::now))).execute(&mut self.conn()?)?;
        Ok(res > 0)
    }

    /// Add a new message to the database.
    /// 
    /// # Arguments
    /// * `msg` - Message object
    /// 
    /// # Returns
    /// * `models::Message` - Message object, with ID and timestamp set
    pub fn add_message(&self, msg: &models::MessageInsert) -> DBResult<models::Message>
    {
        use schema::messages::dsl::*;
        let res = diesel::insert_into(messages)
            .values(msg).get_result(&mut self.conn()?)?;
        Ok(res)
    }

    /// Get a message from the database.
    /// 
    /// # Arguments
    /// * `msg_id` - ID of the message
    /// 
    /// # Returns
    /// * `models::Message` - Message object
    /// * `Err(NotFound)` - Message not found
    pub fn get_message(&self, msg_id: i32) -> DBResult<models::Message>
    {
        use models::*;
        use schema::messages::dsl::*;
        to_db_res(messages.filter(id.eq(msg_id)).first::<Message>(&mut self.conn()?))
    }

    /// Get all messages for a user.
    /// 
    /// # Arguments
    /// * `uid` - User ID
    /// 
    /// # Returns
    /// * `Vec<models::Message>` - List of Message objects
    pub fn get_user_messages(&self, uid: &str) -> DBResult<Vec<models::Message>>
    {
        use models::*;
        use schema::messages::dsl::*;
        Ok(messages.filter(user_id.eq(uid)).load::<Message>(&mut self.conn()?)?)
    }

    /// Set the seen status of a message.
    /// 
    /// # Arguments
    /// * `msg_id` - ID of the message
    /// * `new_status` - New status
    /// 
    /// # Returns
    /// * `Res<bool>` - True if message was found and updated, false if it was not found
    pub fn set_message_seen(&self, msg_id: i32, new_status: bool) -> DBResult<bool>
    {
        use schema::messages::dsl::*;
        let res = diesel::update(messages.filter(id.eq(msg_id)))
            .set(seen.eq(new_status)).execute(&mut self.conn()?)?;
        Ok(res > 0)
    }

    /// Delete a message from the database.
    /// 
    /// # Arguments
    /// * `msg_id` - ID of the message
    /// 
    /// # Returns
    /// * `Res<bool>` - True if message was deleted, false if it was not found
    pub fn del_message(&self, msg_id: i32) -> DBResult<bool>
    {
        use schema::messages::dsl::*;
        let res = diesel::delete(messages.filter(id.eq(msg_id))).execute(&mut self.conn()?)?;
        Ok(res > 0)
    }

}

// -----------------------------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn test_basic_db_ops() -> Result<(), Box<dyn std::error::Error>>
    {
        //let test_db = assert_fs::NamedTempFile::new("test_db.sqlite")?;
        //let db = DB::connect_db_file(&test_db.path())?;
        let db = DB::connect_db_url(":memory:")?;
        db.run_migrations()?;

        let test_video = models::VideoInsert {
            video_hash: "test_hash".to_string(),
            added_by_userid: Some("test_user".to_string()),
            added_by_username: Some("Test User".to_string()), 
            recompression_done: None,
            orig_filename: Some("test.mp4".to_string()),
            total_frames: Some(100),
            duration: Some(10.0),
            fps: Some("1.0".to_string()),
            raw_metadata_all: Some("test".to_string()),
        };

        let test_comment = models::CommentInsert {
            video_hash: "test_hash".to_string(),
            parent_id: None,
            user_id: "test_user".to_string(),
            username: "Test User".to_string(),
            comment: "Test comment".to_string(),
            timecode: None,
            drawing: None,
        };

        let test_message = models::MessageInsert {
            user_id: "test_user".to_string(),
            ref_video_hash: Some("test_hash".to_string()),
            ref_comment_id: None,
            seen: false,
            event_name: "test_event".to_string(),
            message: "Test message".to_string(),
            details: "Test details".to_string(),
        };


        db.add_video(&test_video)?;
        let v = db.get_video("test_hash") ?;
        assert_eq!(v.video_hash, "test_hash");

        db.add_comment(&test_comment)?;
        let c = db.get_video_comments(&v.video_hash)?;
        assert_eq!(c.len(), 1);

        db.add_message(&test_message)?;
        let m = db.get_user_messages(&test_message.user_id)?;
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].message, "Test message");

        //test_db.close()?;
        Ok(())
    }
}
