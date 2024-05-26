use diesel::migration::Migration;
use diesel::prelude::*;
use diesel::r2d2::ConnectionManager;
use diesel::SqliteConnection;
use anyhow::{Context, anyhow};

use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use std::path::Path;
use std::sync::atomic::AtomicBool;

pub mod schema;
pub mod models;
pub mod error;

#[cfg(test)]
pub mod tests;

mod custom_ops;

use error::{DBError, DBResult, EmptyDBResult};

pub type PooledConnection = r2d2::PooledConnection<ConnectionManager<SqliteConnection>>;
pub type Pool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");


#[macro_export]
macro_rules! retry_if_db_locked {
    ($op:expr) => {
        (|| {
            let mut attempt = 1;
            loop {
                let res = $op;
                if res.is_ok() {
                    return res;
                } else {
                    let err_msg = res.as_ref().err().unwrap().to_string();
                    if (attempt <= 8) && err_msg.to_lowercase().contains("locked") {
                        tracing::debug!("DB: '{}, retrying in 100ms (attempt {}/{})", err_msg, attempt, 8);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        attempt += 1;
                        continue;
                    } else {
                        return res;
                    }
                }
            }
        })()
    }
}

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
    pub fn open_db_url(db_url: &str) -> DBResult<Self> {
        let manager = ConnectionManager::<SqliteConnection>::new(db_url);
        let pool = Pool::builder().max_size(16).build(manager).context("Failed to build DB pool")?;
        Ok(DB {
            pool,
            broken_for_test: AtomicBool::new(false),
        })
    }

    /// Connect to SQLite database with a file path
    pub fn open_db_file( db_file: &Path ) -> DBResult<DB> {
        let db_url = format!("sqlite://{}", db_file.to_str().ok_or(anyhow!("Invalid DB file path"))
            .context("Failed to connect DB file")?);
        let res = DB::open_db_url(&db_url);
        res
    }

    /// Get a connection from the pool
    pub fn conn(&self) -> DBResult<PooledConnection> {
        if self.broken_for_test.load(std::sync::atomic::Ordering::Relaxed) {
            let bad_manager = ConnectionManager::<SqliteConnection>::new("sqlite:///dev/urandom");
            let bad_pool = Pool::builder().build(bad_manager).context("TEST ERROR: Failed to build 'broken' DB pool")?;
            return bad_pool.get().map_err(|e| anyhow!("TEST ERROR: Failed to get connection from 'broken' pool: {:?}", e).into());
        }
        let mut conn = self.pool.get().context("Failed to get connection from pool")?;
        diesel::sql_query(r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA wal_autocheckpoint = 1000;
            PRAGMA wal_checkpoint(TRUNCATE);
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 15000;
        "#).execute(&mut conn).context("Failed to set DB pragmas")?;
        Ok(conn)
    }

    /// Return list of any pending migrations
    pub fn pending_migration_names(&self) -> DBResult<Vec<String>> {
        Ok(MigrationHarness::pending_migrations(&mut self.conn()?, MIGRATIONS)
            .map_err(|e| anyhow!("Failed to get migrations: {:?}", e))?
            .iter().map(|m| m.name().to_string()).collect())
    }

    /// Return name of the latest applied migration
    /// or None if no migrations have been applied
    pub fn latest_migration_name(&self) -> DBResult<Option<String>> {
        let applied = MigrationHarness::applied_migrations(&mut self.conn()?)
            .map_err(|e| anyhow!("Failed to get migrations: {:?}", e))?;
        let res = applied.iter().max().and_then(|m| Some(m.to_string()));
        Ok(res)
    }

    /// Run a named migration
    pub fn apply_migration(&self, conn: &mut SqliteConnection, migration_name: &str) -> EmptyDBResult {
        conn.transaction(|conn| {   // uses savepoints instead when needed
            let pending = MigrationHarness::pending_migrations(conn, MIGRATIONS)
                .map_err(|e| anyhow!("Failed to get migrations: {:?}", e))?;
            let migration = pending.iter().find(|m| m.name().to_string() == migration_name)
                .ok_or_else(|| anyhow!("Migration not found: {}", migration_name))?;

            tracing::info!("Applying (Clapshot server) DB migration: {}", migration.name());
            diesel::sql_query("PRAGMA foreign_keys = OFF;").execute(conn)?;
            MigrationHarness::run_migration(conn, &**migration)
                .map_err(|e| anyhow!("Failed to apply migration: {:?}", e))?;
            diesel::sql_query("PRAGMA foreign_keys = ON;").execute(conn)?;
            Ok(())
        })
    }

    /// "Corrupt" the connection for testing so that subsequent queries fail
    pub fn break_db(&self) {
        self.broken_for_test.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

// ---------------- Paging ----------------

pub struct DBPaging {
    pub page_num: u32,
    pub page_size: std::num::NonZeroU32,
}

impl DBPaging {
    pub fn offset(&self) -> i64 {
        (self.page_num * self.page_size.get()) as i64
    }
    pub fn limit(&self) -> i64 {
        self.page_size.get() as i64
    }
}

impl Default for DBPaging {
    fn default() -> Self {
        Self { page_num: 0, page_size: unsafe { std::num::NonZeroU32::new_unchecked(u32::MAX) } }
    }
}


pub trait DbBasicQuery<P, I>: Sized
    where P: std::str::FromStr + Send + Sync + Clone,
          I: Send + Sync,
{
    /// Insert a new object into the database.
    fn insert(conn: &mut PooledConnection, item: &I) -> DBResult<Self>;

    /// Insert multiple objects into the database.
    fn insert_many(conn: &mut PooledConnection, items: &[I]) -> DBResult<Vec<Self>>;

    /// Get a single object by its primary key.
    /// Returns DBError::NotFound() if no object with the given ID was found.
    fn get(conn: &mut PooledConnection, pk: &P) -> DBResult<Self>;

    /// Get multiple objects by their primary keys.
    fn get_many(conn: &mut PooledConnection, ids: &[P]) -> DBResult<Vec<Self>>;

    /// Get all nodes of type Self, with no filtering, paginated.
    fn get_all(conn: &mut PooledConnection, pg: DBPaging) -> DBResult<Vec<Self>>;

    /// Delete a single object from the database.
    fn delete(conn: &mut PooledConnection, id: &P) -> DBResult<bool>;

    /// Delete multiple objects from the database.
    fn delete_many(conn: &mut PooledConnection, ids: &[P]) -> DBResult<usize>;
}

pub trait DbUpdate<P>: Sized
    where P: std::str::FromStr + Send + Sync + Clone,
{
    /// Update objects, replaces the entire object except for the primary key.
    fn update_many(conn: &mut PooledConnection, items: &[Self]) -> DBResult<Vec<Self>>;
}

mod basic_query;
crate::implement_basic_query_traits!(models::User, models::UserInsert, users, String, created.desc());
crate::implement_basic_query_traits!(models::MediaType, models::MediaType, media_types, String, id.desc());
crate::implement_basic_query_traits!(models::MediaFile, models::MediaFileInsert, media_files, String, added_time.desc());
crate::implement_basic_query_traits!(models::Comment, models::CommentInsert, comments, i32, created.desc());
crate::implement_basic_query_traits!(models::Message, models::MessageInsert, messages, i32, created.desc());

crate::implement_update_traits!(models::User, users, String);
crate::implement_update_traits!(models::MediaFile, media_files, String);
crate::implement_update_traits!(models::Comment, comments, i32);
crate::implement_update_traits!(models::Message, messages, i32);



pub trait DbQueryByUser: Sized {
    /// Get all objects of type Self that belong to given user.
    fn get_by_user(conn: &mut PooledConnection, uid: &str, pg: DBPaging) -> DBResult<Vec<Self>>;
}
crate::implement_query_by_user_traits!(models::User, users, id, created.desc());
crate::implement_query_by_user_traits!(models::MediaFile, media_files, user_id, added_time.desc());
crate::implement_query_by_user_traits!(models::Comment, comments, user_id, created.desc());
crate::implement_query_by_user_traits!(models::Message, messages, user_id, created.desc());



pub trait DbQueryByMediaFile: Sized {
    /// Get all objects of type Self that are linked to given media file.
    fn get_by_media_file(conn: &mut PooledConnection, vid: &str, pg: DBPaging) -> DBResult<Vec<Self>>;
}
crate::implement_query_by_media_file_traits!(models::Comment, comments, media_file_id, created.desc());
crate::implement_query_by_media_file_traits!(models::Message, messages, media_file_id, created.desc());
