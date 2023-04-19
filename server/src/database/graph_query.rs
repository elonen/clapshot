#[macro_export]
macro_rules! implement_graph_query_traits {
    ($model:ty, $table:ident, $table_id_type:ty, $edge_from_col:ident, $edge_to_col:ident) => {

        impl DbGraphQuery for $model
        {
            fn graph_get_by_parent(db: &DB, parent_id: GraphObjId, edge_type: Option<&str>)
                -> DBResult<Vec<EdgeAndObj<Self>>>
            {
                macro_rules! get_by_parent {
                    ($db:expr, $edge_type:expr, $to_mod:ident, $to_col:ident, $to_id:expr, $from_mod:ident, $from_col:ident, $from_res_t:ty) =>
                    {
                        {
                            type ResultType = (models::PropEdge, Option<$from_res_t>);
                            let et = $edge_type;
                            {
                                use schema::prop_edges::dsl::*;
                                use schema::$from_mod::dsl::id as from__id;

                                let from_table = diesel::alias!(schema::$from_mod as f);
                                let q = prop_edges
                                    .left_join(from_table.on(from_table.field(from__id).nullable().eq($from_col)))
                                    .into_boxed();

                                // If edge type is given, filter by it
                                let q = if let Some(et) = et {
                                    q.filter($to_col.eq($to_id).and($from_col.is_not_null()).and(edge_type.eq(et)))
                                } else {
                                    q.filter($to_col.eq($to_id).and($from_col.is_not_null()))
                                }.order(sort_order.asc());

                                // We shouldn't get NULLs because of the filter above, but just in case...
                                let res = q.load::<ResultType>(&mut $db.conn()?)?;
                                let res = res.into_iter().filter_map(|(e, o)| {
                                    if let Some(o) = o {
                                        Some(EdgeAndObj { edge: e, obj: o })
                                    } else {
                                        tracing::error!("BUG: unexpected NULL in DB query result. SQL 'where' is faulty.");
                                        None
                                    }
                                }).collect();
                                Ok(res)
                            }
                        }
                    }
                }
                match parent_id {
                    GraphObjId::Video(pid) => get_by_parent!(db, edge_type, videos, to_video, pid, $table, $edge_from_col, $model),
                    GraphObjId::Node(pid) => get_by_parent!(db, edge_type, prop_nodes, to_node, pid, $table, $edge_from_col, $model),
                    GraphObjId::Comment(pid) => get_by_parent!(db, edge_type, comments, to_comment, pid, $table, $edge_from_col, $model),
                }
            }

            fn graph_get_by_child(db: &DB, child_id: GraphObjId, edge_type: Option<&str>)
                -> DBResult<Vec<EdgeAndObj<Self>>>
            {
                macro_rules! get_by_child {
                    ($db:expr, $edge_type:expr, $from_mod:ident, $from_col:ident, $from_id:expr, $to_mod:ident, $to_col:ident, $to_res_t:ty) => {
                        {
                            type ResultType = (models::PropEdge, Option<$to_res_t>);
                            let et = $edge_type;
                            {
                                use schema::prop_edges::dsl::*;
                                use schema::$to_mod::dsl::id as to__id;

                                let to_table = diesel::alias!(schema::$to_mod as t);
                                let q = prop_edges
                                    .left_join(to_table.on(to_table.field(to__id).nullable().eq($to_col)))
                                    .into_boxed();

                                // If edge type is given, filter by it
                                let q = if let Some(et) = et {
                                    q.filter($from_col.eq($from_id).and($to_col.is_not_null()).and(edge_type.eq(et)))
                                } else {
                                    q.filter($from_col.eq($from_id).and($to_col.is_not_null()))
                                }.order(sort_order.asc());

                                // We shouldn't get NULLs because of the filter above, but just in case...
                                let res = q.load::<ResultType>(&mut $db.conn()?)?;
                                let res = res.into_iter().filter_map(|(e, o)| {
                                    if let Some(o) = o {
                                        Some(EdgeAndObj { edge: e, obj: o })
                                    } else {
                                        tracing::error!("BUG: unexpected NULL in DB query result. SQL 'where' is faulty.");
                                        None
                                    }
                                }).collect();
                                Ok(res)
                            }
                        }
                    }
                }
                match child_id {
                    GraphObjId::Video(pid) => get_by_child!(db, edge_type, videos, from_video, pid, $table, $edge_to_col, $model),
                    GraphObjId::Node(pid) => get_by_child!(db, edge_type, prop_nodes, from_node, pid, $table, $edge_to_col, $model),
                    GraphObjId::Comment(pid) => get_by_child!(db, edge_type, comments, from_comment, pid, $table, $edge_to_col, $model),
                }
            }

            fn graph_get_parentless(db: &DB, edge_type: Option<&str>)
                -> DBResult<Vec<$model>>
            {
                let et = edge_type; {
                    use schema::$table::dsl::*;
                    use schema::$table::dsl::id as self_id;
                    use schema::prop_edges::dsl::*;

                    Ok(if let Some(et) = et {
                        $table.left_join(prop_edges.on($edge_from_col
                            .eq(self_id.nullable())
                            .and(edge_type.eq(et))))
                        .filter($edge_from_col.is_null())
                        .select($table::all_columns())
                        .load::<$model>(&mut db.conn()?)?
                    } else {
                        $table.left_join(prop_edges.on($edge_from_col
                            .eq(self_id.nullable()))).into_boxed()
                        .filter($edge_from_col.is_null())
                        .select($table::all_columns())
                        .load::<$model>(&mut db.conn()?)?
                    })
                }
            }

            fn graph_get_childless(db: &DB, edge_type: Option<&str>)
                -> DBResult<Vec<$model>>
            {
                let et = edge_type; {
                    use schema::$table::dsl::*;
                    use schema::$table::dsl::id as self_id;
                    use schema::prop_edges::dsl::*;

                    Ok(if let Some(et) = et {
                        $table.left_join(prop_edges.on($edge_to_col
                            .eq(self_id.nullable())
                            .and(edge_type.eq(et))))
                        .filter($edge_to_col.is_null())
                        .select($table::all_columns())
                        .load::<$model>(&mut db.conn()?)?
                    } else {
                        $table.left_join(prop_edges.on($edge_to_col
                            .eq(self_id.nullable()))).into_boxed()
                        .filter($edge_to_col.is_null())
                        .select($table::all_columns())
                        .load::<$model>(&mut db.conn()?)?
                    })
                }
            }

        }
    }
}
