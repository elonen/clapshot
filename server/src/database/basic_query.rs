#[macro_export]
macro_rules! implement_basic_query_traits {
    ($model:ty, $insert_model:ty, $table:ident, $pk_type:ty) => {

        impl DbBasicQuery<$pk_type, $insert_model> for $model {

            /// Insert a new object into the database.
            fn add(db: &DB, item: &$insert_model) -> DBResult<Self> {
                use schema::$table::dsl::*;
                to_db_res(diesel::insert_into($table).values(item).get_result(&mut db.conn()?))
            }

            /// Insert multiple objects into the database.
            fn add_many(db: &DB, items: &[$insert_model]) -> DBResult<Vec<Self>> {
                items.iter().map(|i| Self::add(db, i)).collect()
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
            ///
            /// # Arguments
            /// * `db` - Database connection
            /// * `page` - Page number (0 = first page)
            /// * `page_size` - Number of items per page
            fn get_all(db: &DB, page: u64, page_size: u64) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;

                let page = std::cmp::max(1, page);
                let page_size = std::cmp::max(1, page_size);
                let offset = (page - 1) * page_size;

                to_db_res($table
                    .order(id.asc())
                    .offset(offset as i64)
                    .limit(page_size as i64)
                    .load::<$model>(&mut db.conn()?))
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

            fn get_by_user(db: &DB, uid: &str, page: u64, page_size: u64) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;

                let page = std::cmp::max(1, page);
                let page_size = std::cmp::max(1, page_size);
                let offset = (page - 1) * page_size;

                to_db_res($table
                    .filter(user_id.eq(uid))
                    .order($order_by)
                    .offset(offset as i64)
                    .limit(page_size as i64)
                    .load::<$model>(&mut db.conn()?))
            }
        }
    }
}

#[macro_export]
macro_rules! implement_query_by_video_traits {
    ($model:ty, $table:ident, $video_col:ident, $order_by:expr) => {

        impl DbQueryByVideo for $model {

            fn get_by_video(db: &DB, vid: &str, page: u64, page_size: u64) -> DBResult<Vec<Self>> {
                use schema::$table::dsl::*;

                let page = std::cmp::max(1, page);
                let page_size = std::cmp::max(1, page_size);
                let offset = (page - 1) * page_size;

                to_db_res($table
                    .filter($video_col.eq(vid))
                    .order($order_by)
                    .offset(offset as i64)
                    .limit(page_size as i64)
                    .load::<$model>(&mut db.conn()?))
            }
        }
    }
}
