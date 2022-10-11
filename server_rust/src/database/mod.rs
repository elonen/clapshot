#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager};
use diesel::SqliteConnection;

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::offset::Local;

pub mod schema;
pub mod models;

pub type Pool = diesel::r2d2::Pool<ConnectionManager<SqliteConnection>>;
type PooledConnection = r2d2::PooledConnection<ConnectionManager<SqliteConnection>>;

type Res<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type EmptyResult = Res<()>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();


pub fn connect_db( db_file: &Path ) -> Res<Pool>
{
    let db_url = format!("sqlite://{}", db_file.to_str().ok_or("Invalid DB file path")?);
    let manager = ConnectionManager::<SqliteConnection>::new(db_url);
    let pool = Pool::builder().build(manager)?;
    Ok(pool)
}


pub fn create_tables( conn: &mut PooledConnection ) -> EmptyResult
{
    conn.run_pending_migrations(MIGRATIONS).unwrap();
    Ok(())
}

pub fn add_test_video( conn: &mut PooledConnection ) -> EmptyResult
{
    use models::*;
    use schema::videos::dsl::*;

    let new_video = Video {
        id: 0,
        video_hash: "test_hash".to_string(),
        added_by_userid: Some("test_user".to_string()),
        added_by_username: Some("Test User".to_string()), 
        added_time: Local::now().naive_local(),
        recompression_done: None,
        orig_filename: Some("test.mp4".to_string()),
        total_frames: Some(100),
        duration: Some(10.0),
        fps: Some("1.0".to_string()),
        raw_metadata_all: Some("test".to_string()),
    };

    diesel::insert_into(videos)
        .values(&new_video)
        .execute(conn)?;

    Ok(())
}
pub fn get_test_video( conn: &mut PooledConnection ) -> Res<models::Video>
{
    use models::*;
    use schema::videos::dsl::*;

    let v = videos
        .filter(video_hash.eq("test_hash"))
        .first::<Video>(conn)?;

    Ok(v)
}

pub fn add_test_comment( conn: &mut PooledConnection ) -> EmptyResult
{
    use models::*;
    use schema::comments::dsl::*;

    let new_comment = Comment {
        id: 0,
        video_hash: "test_hash".to_string(),
        parent_id: None,
        created: Local::now().naive_local(),
        edited: None,
        user_id: "test_user".to_string(),
        username: "Test User".to_string(),
        comment: "Test comment".to_string(),
        timecode: None,
        drawing: None,
    };

    diesel::insert_into(comments)
        .values(&new_comment)
        .execute(conn)?;

    Ok(())
}

pub fn get_test_comment( conn: &mut PooledConnection ) -> Res<models::Comment>
{
    use models::*;
    use schema::comments::dsl::*;
    let c = comments
        .first::<Comment>(conn)?;
    Ok(c)
}

pub fn add_test_message( conn: &mut PooledConnection ) -> EmptyResult
{
    use models::*;
    use schema::messages::dsl::*;

    let new_message = Message {
        id: 0,
        user_id: "test_user".to_string(),
        created: Local::now().naive_local(),
        seen: false,
        ref_video_hash: Some("test_hash".to_string()),
        ref_comment_id: None,
        event_name: "test_event".to_string(),
        message: "Test message".to_string(),
        details: "Test details".to_string(),
    };

    diesel::insert_into(messages)
        .values(&new_message)
        .execute(conn)?;

    Ok(())
}

pub fn get_test_message( conn: &mut PooledConnection ) -> Res<models::Message>
{
    use models::*;
    use schema::messages::dsl::*;
    let m = messages
        .first::<Message>(conn)?;
    Ok(m)
}

// -----------------------------------------------------------------------------------------------

/// Add a new video to the database.
/// 
/// # Arguments
/// * `video` - Video object
/// 
/// # Returns
/// * `sql.Integer` - ID of the new video
pub fn add_video( conn: &mut PooledConnection, video: &models::Video ) -> Res<i32>
{
    use models::*;
    use schema::videos::dsl::*;
    let res = diesel::insert_into(videos)
        .values(video).returning(id).get_result(conn)?;
    Ok(res)
}

/// Set the recompressed flag for a video.
/// 
/// # Arguments
/// * `video_hash` - Hash (unique identifier) of the video
pub fn set_video_recompressed( conn: &mut PooledConnection, video_hash: &str ) -> EmptyResult
{
    use models::*;
    use schema::videos::dsl::*;
    diesel::update(videos.filter(video_hash.eq(video_hash)))
        .set(recompression_done.eq(Local::now().naive_local()))
        .execute(conn)?;
    Ok(())
}


/// Get a video from the database.
/// 
/// # Arguments
/// * `video_hash` - Hash (unique identifier) of the video
///
/// # Returns
/// * `models::Video` - Video object
/// * `Err(NotFound)` - Video not found
pub fn get_video( conn: &mut PooledConnection, video_hash: &str ) -> Res<models::Video>
{
    use models::*;
    use schema::videos::dsl::*;
    let res = videos.filter(video_hash.eq(video_hash)).first::<Video>(conn)?;
    Ok(res)
}


/// Delete a video and all its comments from the database.
///     
/// # Arguments
/// * `video_hash` - Hash (unique identifier) of the video
/// 
/// # Returns
/// * `EmptyResult`
/// * `Err(NotFound)` - Video not found
pub fn del_video_and_comments( conn: &mut PooledConnection, vh: &str ) -> EmptyResult
{
    use schema::videos::dsl as sv;
    use schema::comments::dsl as sc;
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
pub fn get_all_user_videos( conn: &mut PooledConnection, user_id: &str ) -> Res<Vec<models::Video>>
{
    use models::*;
    use schema::videos::dsl::*;
    Ok(videos.filter(added_by_userid.eq(user_id)).load::<Video>(conn)?)
}


/// Add a new comment on a video.
/// 
/// # Arguments
/// * `comment` - Comment object
/// 
/// # Returns
/// * `i32` - ID of the new comment
pub fn add_comment( conn: &mut PooledConnection, cmt: &models::Comment ) -> Res<i32>
{
    use models::*;
    use schema::comments::dsl::*;
    let res = diesel::insert_into(comments)
        .values(cmt).returning(id).get_result(conn)?;
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
pub fn get_comment( conn: &mut PooledConnection, comment_id: i32 ) -> Res<models::Comment>
{
    use models::*;
    use schema::comments::dsl::*;
    let res = comments.filter(id.eq(comment_id)).first::<Comment>(conn)?;
    Ok(res)
}


/// Get all comments for a video.
/// 
/// # Arguments
/// * `vh` - Hash (unique identifier) of the video
/// 
/// # Returns
/// * `Vec<models::Comment>` - List of Comment objects
pub fn get_video_comments( conn: &mut PooledConnection, vh: &str ) -> Res<Vec<models::Comment>>
{
    use models::*;
    use schema::comments::dsl::*;
    Ok(comments.filter(video_hash.eq(vh)).load::<Comment>(conn)?)
}

/// Delete a comment from the database.
/// 
/// # Arguments
/// * `comment_id` - ID of the comment
/// 
/// # Returns
/// * `Res<bool>` - True if comment was deleted, false if it was not found
pub fn del_comment( conn: &mut PooledConnection, comment_id: i32 ) -> Res<bool>
{
    use schema::comments::dsl::*;
    let res = diesel::delete(comments.filter(id.eq(comment_id))).execute(conn)?;
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
pub fn edit_comment( conn: &mut PooledConnection, comment_id: i32, new_comment: &str ) -> Res<bool>
{
    use schema::comments::dsl::*;
    let res = diesel::update(comments.filter(id.eq(comment_id)))
        .set((comment.eq(new_comment), edited.eq(diesel::dsl::now))).execute(conn)?;
    Ok(res > 0)
}


/// Add a new message to the database.
/// 
/// # Arguments
/// * `msg` - Message object
/// 
/// # Returns
/// * `models::Message` - Message object, with ID and timestamp set
pub fn add_message( conn: &mut PooledConnection, msg: &models::Message ) -> Res<models::Message>
{
    use models::*;
    use schema::messages::dsl::*;
    let res = diesel::insert_into(messages)
        .values(msg).get_result(conn)?;
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
pub fn get_message( conn: &mut PooledConnection, msg_id: i32 ) -> Res<models::Message>
{
    use models::*;
    use schema::messages::dsl::*;
    let res = messages.filter(id.eq(msg_id)).first::<Message>(conn)?;
    Ok(res)
}


/// Get all messages for a user.
/// 
/// # Arguments
/// * `user_id` - User ID
/// 
/// # Returns
/// * `Vec<models::Message>` - List of Message objects
pub fn get_user_messages( conn: &mut PooledConnection, user_id: &str ) -> Res<Vec<models::Message>>
{
    use models::*;
    use schema::messages::dsl::*;
    Ok(messages.filter(user_id.eq(user_id)).load::<Message>(conn)?)
}


/// Set the seen status of a message.
/// 
/// # Arguments
/// * `msg_id` - ID of the message
/// * `new_status` - New status
/// 
/// # Returns
/// * `Res<bool>` - True if message was found and updated, false if it was not found
pub fn set_message_seen( conn: &mut PooledConnection, msg_id: i32, new_status: bool ) -> Res<bool>
{
    use schema::messages::dsl::*;
    let res = diesel::update(messages.filter(id.eq(msg_id)))
        .set(seen.eq(new_status)).execute(conn)?;
    Ok(res > 0)
}


/// Delete a message from the database.
/// 
/// # Arguments
/// * `msg_id` - ID of the message
/// 
/// # Returns
/// * `Res<bool>` - True if message was deleted, false if it was not found
pub fn del_message( conn: &mut PooledConnection, msg_id: i32 ) -> Res<bool>
{
    use schema::messages::dsl::*;
    let res = diesel::delete(messages.filter(id.eq(msg_id))).execute(conn)?;
    Ok(res > 0)
}

// -----------------------------------------------------------------------------------------------





#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn test_basic_db_ops() -> EmptyResult
    {
        let test_db = assert_fs::NamedTempFile::new("test_db.sqlite")?;
        let pool = connect_db(&test_db.path())?;
        let mut conn = pool.get()?;
        create_tables(&mut conn)?;
        add_test_video(&mut conn)?;
        let v = get_test_video(&mut conn)?;
        assert_eq!(v.video_hash, "test_hash");

        add_test_comment(&mut conn)?;
        get_test_comment(&mut conn)?;

        add_test_message(&mut conn)?;
        get_test_message(&mut conn)?;

        test_db.close()?;
        Ok(())
    }
}
