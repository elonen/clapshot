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
