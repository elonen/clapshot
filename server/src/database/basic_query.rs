#[macro_export]
macro_rules! implement_basic_query_traits {
    ($model:ty, $insert_model:ty, $table:ident, $pk_type:ty, $order_by:expr) => {

        impl DbBasicQuery<$pk_type, $insert_model> for $model {

            /// Insert a new object into the database.
            fn insert(conn: &mut PooledConnection, item: &$insert_model) -> DBResult<Self> {
                use schema::$table::dsl::*;
                to_db_res(retry_if_db_locked!(diesel::insert_into($table).values(item).get_result(conn)))
            }

            /// Insert multiple objects into the database.
            fn insert_many(conn: &mut PooledConnection, items: &[$insert_model]) -> DBResult<Vec<Self>> {
                items.iter().map(|i| Self::insert(conn, i)).collect()
            }

            /// Get a single object by its primary key.
            fn get(conn: &mut PooledConnection, pk: &$pk_type) -> DBResult<Self>
            {
                use schema::$table::dsl::*;
                to_db_res(retry_if_db_locked!({ $table.filter(id.eq(pk)).first::<$model>(conn) }))
            }

            /// Get multiple objects by their primary keys.
            fn get_many(conn: &mut PooledConnection, ids: &[$pk_type]) -> DBResult<Vec<Self>>
            {
                use schema::$table::dsl::*;
                to_db_res(retry_if_db_locked!({ $table.filter(id.eq_any(ids)).load::<$model>(conn) }))
            }

            /// Get all nodes of type Self, with no filtering, paginated.
            fn get_all(conn: &mut PooledConnection, pg: DBPaging) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;
                to_db_res(retry_if_db_locked!({
                    $table
                        .order($order_by)
                        .then_order_by(id.asc())
                        .offset(pg.offset())
                        .limit(pg.limit())
                        .load::<$model>(conn)
                }))
            }

            /// Delete a single object from the database.
            fn delete(conn: &mut PooledConnection, pk: &$pk_type) -> DBResult<bool>
            {
                use schema::$table::dsl::*;
                to_db_res(retry_if_db_locked!({
                    diesel::delete($table.filter(id.eq(pk))).execute(conn).map(|n_rows| n_rows>0)
                }))
            }

            /// Delete multiple objects from the database.
            /// Returns the number of objects deleted.
            fn delete_many(conn: &mut PooledConnection, ids: &[$pk_type]) -> DBResult<usize>
            {
                use schema::$table::dsl::*;
                to_db_res(retry_if_db_locked!({
                    diesel::delete($table.filter(id.eq_any(ids))).execute(conn)
                }))
            }
        }
    }
}

#[macro_export]
macro_rules! implement_update_traits {
    ($model:ty, $table:ident, $pk_type:ty) => {

        impl DbUpdate<$pk_type> for $model {
            /// Update objects, replaces the entire object except for the primary key.
            fn update_many(conn: &mut PooledConnection, items: &[Self]) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;
                let mut res: Vec<Self> = Vec::with_capacity(items.len());
                for it in items {
                    res.push(retry_if_db_locked!(diesel::update($table.filter(id.eq(&it.id))).set(it).get_result(conn))?);
                }
                Ok(res)
            }
        }
    }
}

#[macro_export]
macro_rules! implement_query_by_user_traits {
    ($model:ty, $table:ident, $user_field:ident, $order_by:expr) => {

        impl DbQueryByUser for $model {

            fn get_by_user(conn: &mut PooledConnection, uid: &str, pg: DBPaging) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;
                to_db_res(retry_if_db_locked!({
                    $table
                        .filter($user_field.eq(uid))
                        .order($order_by)
                        .then_order_by(id.asc())
                        .offset(pg.offset())
                        .limit(pg.limit())
                        .load::<$model>(conn)
                }))
            }
        }
    }
}

#[macro_export]
macro_rules! implement_query_by_media_file_traits {
    ($model:ty, $table:ident, $media_col:ident, $order_by:expr) => {

        impl DbQueryByMediaFile for $model {

            fn get_by_media_file(conn: &mut PooledConnection, vid: &str, pg: DBPaging) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;
                to_db_res(retry_if_db_locked!({
                    $table
                        .filter($media_col.eq(vid))
                        .order($order_by)
                        .then_order_by(id.asc())
                        .offset(pg.offset())
                        .limit(pg.limit())
                        .load::<$model>(conn)
                }))
            }
        }
    }
}
