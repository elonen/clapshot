use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager};
use diesel::SqliteConnection;
use anyhow::{Context, anyhow};

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use std::path::{Path};
use std::sync::atomic::AtomicBool;

use chrono::offset::Local;

pub mod schema;
pub mod models;
pub mod error;

#[cfg(test)]
pub mod tests;

use error::{DBError, DBResult, EmptyDBResult};

pub type Pool = diesel::r2d2::Pool<ConnectionManager<SqliteConnection>>;
type PooledConnection = r2d2::PooledConnection<ConnectionManager<SqliteConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");


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
    broken_for_test: AtomicBool,
}


impl DB {

    /// Connect to SQLite database with an URL (use this for memory databases)
    pub fn connect_db_url( db_url: &str ) -> DBResult<DB> {
        let manager = ConnectionManager::<SqliteConnection>::new(db_url);
        let pool = Pool::builder().max_size(1).build(manager).context("Failed to build DB pool")?;

        let db = DB { pool: pool, broken_for_test: AtomicBool::new(false) };

        diesel::sql_query("PRAGMA foreign_keys = ON;")
            .execute(&mut db.conn()?)
            .context("Failed to enable foreign keys")?;

        Ok(db)
    }

    /// Connect to SQLite database with a file path
    pub fn connect_db_file( db_file: &Path ) -> DBResult<DB> {
        let db_url = format!("sqlite://{}", db_file.to_str().ok_or(anyhow!("Invalid DB file path"))
            .context("Failed to connect DB file")?);
        DB::connect_db_url(&db_url)
    }


    /// Get a connection from the pool
    pub fn conn(&self) ->  DBResult<PooledConnection> {
        if self.broken_for_test.load(std::sync::atomic::Ordering::Relaxed) {
            let bad_pool = Pool::builder().build(ConnectionManager::<SqliteConnection>::new("sqlite:///dev/urandom")).context("Failed to build 'broken' DB pool")?;
            return bad_pool.get().map_err(|e| anyhow!("Failed to get connection from pool: {:?}", e).into());
        };
        self.pool.get().map_err(|e| anyhow!("Failed to get connection from pool: {:?}", e).into())
    }

    // Check if database is up-to-date compared to the embedded migrations
    pub fn migrations_needed(&self) -> DBResult<bool> {
        let mut conn = self.conn()?;
        MigrationHarness::has_pending_migration(&mut conn, MIGRATIONS)
            .map_err(|e| anyhow!("Failed to check migrations: {:?}", e).into())
    }

    /// Run DB migrations (or create DB if empty)
    pub fn run_migrations(&self) -> EmptyDBResult
    {
        let mut conn = self.conn()?;
        diesel::sql_query("PRAGMA foreign_keys = OFF;").execute(&mut conn)?;
        let migr = conn.run_pending_migrations(MIGRATIONS).map_err(|e| anyhow!("Failed to apply migrations: {:?}", e))?;
        for m in migr { tracing::info!("Applied DB migration: {}", m); }
        diesel::sql_query("PRAGMA foreign_keys = ON;").execute(&mut conn)?;
        Ok(())
    }

    /// "Corrupt" the connection for testing so that subsequent queries fail
    pub fn break_db(&self) {
        self.broken_for_test.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}



pub trait DbBasicQuery<P, I>: Sized
    where P: std::str::FromStr + Send + Sync + Clone,
          I: Send + Sync,
{
    /// Insert a new object into the database.
    fn add(db: &DB, item: &I) -> DBResult<Self>;

    /// Insert multiple objects into the database.
    fn add_many(db: &DB, items: &[I]) -> DBResult<Vec<Self>>;

    /// Get a single object by its primary key.
    /// Returns None if no object with the given ID was found.
    fn get(db: &DB, pk: &P) -> DBResult<Self>;

    /// Get multiple objects by their primary keys.
    fn get_many(db: &DB, ids: &[P]) -> DBResult<Vec<Self>>;

    /// Get all nodes of type Self, with no filtering, paginated.
    ///
    /// # Arguments
    /// * `db` - Database connection
    /// * `page` - Page number (0 = first page)
    /// * `page_size` - Number of items per page
    fn get_all(db: &DB, page: u64, page_size: u64) -> DBResult<Vec<Self>>;

    /// Delete a single object from the database.
    fn delete(db: &DB, id: &P) -> DBResult<bool>;

    /// Delete multiple objects from the database.
    fn delete_many(db: &DB, ids: &[P]) -> DBResult<usize>;
}

mod basic_query;
crate::implement_basic_query_traits!(models::Video, models::VideoInsert, videos, String);
crate::implement_basic_query_traits!(models::Comment, models::CommentInsert, comments, i32);
crate::implement_basic_query_traits!(models::Message, models::MessageInsert, messages, i32);
crate::implement_basic_query_traits!(models::PropNode, models::PropNodeInsert, prop_nodes, i32);
crate::implement_basic_query_traits!(models::PropEdge, models::PropEdgeInsert, prop_edges, i32);



pub trait DbQueryByUser: Sized {
    /// Get all objects of type Self that belong to given user.
    ///
    /// # Arguments
    /// * `db` - Database connection
    /// * `uid` - User ID
    /// * `page` - Page number (0 = first page)
    /// * `page_size` - Number of items per page
    fn get_by_user(db: &DB, uid: &str, page: u64, page_size: u64) -> DBResult<Vec<Self>>;
}
crate::implement_query_by_user_traits!(models::Video, videos, added_time.desc());
crate::implement_query_by_user_traits!(models::Comment, comments, created.desc());
crate::implement_query_by_user_traits!(models::Message, messages, created.desc());



pub trait DbQueryByVideo: Sized {
    /// Get all objects of type Self that are linked to given video.
    ///
    /// # Arguments
    /// * `db` - Database connection
    /// * `vid` - Video ID
    /// * `page` - Page number (0 = first page)
    /// * `page_size` - Number of items per page
    fn get_by_video(db: &DB, vid: &str, page: u64, page_size: u64) -> DBResult<Vec<Self>>;
}
crate::implement_query_by_video_traits!(models::Comment, comments, video_id, created.desc());
crate::implement_query_by_video_traits!(models::Message, messages, ref_video_id, created.desc());



pub enum GraphObjId<'a> {
    Video(&'a str),
    Node(&'a i32),
    Comment(&'a i32)
}
pub struct EdgeAndObj<T> {
    pub edge: models::PropEdge,
    pub obj: T
}

pub trait DbGraphQuery: Sized {

    /// Get nodes of type Self that have edges pointing to the given node.
    /// If `edge_type` is Some, only edges of that type are considered.
    fn graph_get_by_parent(db: &DB, parent_id: GraphObjId, edge_type: Option<&str>)
        -> DBResult<Vec<EdgeAndObj<Self>>>;

    /// Get nodes of type Self that have edges pointing to it from the given node.
    /// If edge_type is Some, only edges of that type are considered.
    fn graph_get_by_child(db: &DB, child_id: GraphObjId, edge_type: Option<&str>)
        -> DBResult<Vec<EdgeAndObj<Self>>>;

    /// Get nodes of type Self that have no edges pointing away from them.
    /// If `edge_type` is Some, only edges of that type are considered.
    fn graph_get_parentless(db: &DB, edge_type: Option<&str>)
        -> DBResult<Vec<Self>>;

    /// Get nodes of type Self that have no edges pointing to them.
    /// If `edge_type` is Some, only edges of that type are considered.
    fn graph_get_childless(db: &DB, edge_type: Option<&str>)
        -> DBResult<Vec<Self>>;
}

mod graph_query;
crate::implement_graph_query_traits!(models::Video, videos, String, from_video, to_video);
crate::implement_graph_query_traits!(models::PropNode, prop_nodes, i32, from_node, to_node);
crate::implement_graph_query_traits!(models::Comment, comments, i32, from_comment, to_comment);


// --------------------------------------------------------

impl models::Video {

    /// Set the recompressed flag for a video.
    ///
    /// # Arguments
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
}


impl models::PropNode {

    /// Get (some) nodes from the prop graph database, filtered by node type.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `node_type` - Node type to filter by.
    /// * `node_ids` - Optional list of node IDs to filter by. If None, consider all nodes.
    pub fn get_by_type(db: &DB, node_type: &str, node_ids: Option<&[i32]>) -> DBResult<Vec<models::PropNode>>
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

    /// Get (some) edges from the prop graph database, filtered by edge type.
    ///
    /// # Arguments
    /// * `db` - Database
    /// * `edge_type` - Edge type to filter by.
    /// * `edge_ids` - Optional list of edge IDs to filter by. If None, consider all edges.
    pub fn get_by_type(db: &DB, edge_type: &str, edge_ids: Option<&[i32]>) -> DBResult<Vec<models::PropEdge>>
    {
        let et = edge_type;
        {
            use schema::prop_edges::dsl::*;
            let q = prop_edges.filter(edge_type.eq(et)).into_boxed();
            let q = if let Some(edge_ids) = edge_ids { q.filter(id.eq_any(edge_ids)) } else { q };
            to_db_res(q.load::<models::PropEdge>(&mut db.conn()?))
        }
    }
}
