#[macro_export]
macro_rules! implement_basic_query_traits {
    ($model:ty, $insert_model:ty, $table:ident, $pk_type:ty, $order_by:expr) => {

        impl DbBasicQuery<$pk_type, $insert_model> for $model {

            /// Insert a new object into the database.
            fn insert(db: &DB, item: &$insert_model) -> DBResult<Self> {
                use schema::$table::dsl::*;
                to_db_res(diesel::insert_into($table).values(item).get_result(&mut db.conn()?))
            }

            /// Insert multiple objects into the database.
            fn insert_many(db: &DB, items: &[$insert_model]) -> DBResult<Vec<Self>> {
                items.iter().map(|i| Self::insert(db, i)).collect()
            }

            /// Get a single object by its primary key.
            fn get(db: &DB, pk: &$pk_type) -> DBResult<Self>
            {
                use schema::$table::dsl::*;
                to_db_res($table.filter(id.eq(pk)).first::<$model>(&mut db.conn()?))
            }

            /// Get multiple objects by their primary keys.
            fn get_many(db: &DB, ids: &[$pk_type]) -> DBResult<Vec<Self>>
            {
                use schema::$table::dsl::*;
                to_db_res($table.filter(id.eq_any(ids)).load::<$model>(&mut db.conn()?))
            }

            /// Get all nodes of type Self, with no filtering, paginated.
            fn get_all(db: &DB, pg: DBPaging) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;
                to_db_res($table
                    .order($order_by)
                    .then_order_by(id.asc())
                    .offset(pg.offset())
                    .limit(pg.limit())
                    .load::<$model>(&mut db.conn()?))
            }

            /// Update objects, replaces the entire object except for the primary key.
            fn update_many(db: &DB, items: &[Self]) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;
                let conn = &mut db.conn()?;
                let mut res: Vec<Self> = Vec::with_capacity(items.len());
                for it in items {
                    res.push(diesel::update($table.filter(id.eq(&it.id))).set(it).get_result(conn)?);
                }
                Ok(res)
            }

            /// Delete a single object from the database.
            fn delete(db: &DB, pk: &$pk_type) -> DBResult<bool>
            {
                use schema::$table::dsl::*;
                let res = diesel::delete($table.filter(id.eq(pk))).execute(&mut db.conn()?)?;
                Ok(res > 0)
            }

            /// Delete multiple objects from the database.
            /// Returns the number of objects deleted.
            fn delete_many(db: &DB, ids: &[$pk_type]) -> DBResult<usize>
            {
                use schema::$table::dsl::*;
                Ok(diesel::delete($table.filter(id.eq_any(ids))).execute(&mut db.conn()?)?)
            }
        }
    }
}

#[macro_export]
macro_rules! implement_query_by_user_traits {
    ($model:ty, $table:ident, $order_by:expr) => {

        impl DbQueryByUser for $model {

            fn get_by_user(db: &DB, uid: &str, pg: DBPaging) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;
                to_db_res($table
                    .filter(user_id.eq(uid))
                    .order($order_by)
                    .then_order_by(id.asc())
                    .offset(pg.offset())
                    .limit(pg.limit())
                    .load::<$model>(&mut db.conn()?))
            }
        }
    }
}

#[macro_export]
macro_rules! implement_query_by_video_traits {
    ($model:ty, $table:ident, $video_col:ident, $order_by:expr) => {

        impl DbQueryByVideo for $model {

            fn get_by_video(db: &DB, vid: &str, pg: DBPaging) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;

                to_db_res($table
                    .filter($video_col.eq(vid))
                    .order($order_by)
                    .then_order_by(id.asc())
                    .offset(pg.offset())
                    .limit(pg.limit())
                    .load::<$model>(&mut db.conn()?))
            }
        }
    }
}
