use std::{sync::atomic::Ordering::Relaxed, path::Path};
use anyhow::Context;
use tonic::{Request, Response, Status};
use crate::{api_server::{server_state::ServerState, SendTo}, database::{DbBasicQuery, DbQueryByUser, DbGraphQuery, DbQueryByVideo}, grpc::{grpc_impl_helpers::{rpc_expect_field, paged_vec}}, client_cmd};
use crate::grpc::db_models::proto_msg_type_to_event_name;
use crate::database::models;

use lib_clapshot_grpc::{proto::{self}, RpcResult, GrpcBindAddr, run_grpc_server};
use lib_clapshot_grpc::proto::org;

pub struct OrganizerOutboundImpl {
    server: ServerState,
}

// Implement RCP methods for Organizer -> Server

#[tonic::async_trait]
impl org::organizer_outbound_server::OrganizerOutbound for OrganizerOutboundImpl
{
    async fn handshake(&self, _req: tonic::Request<org::OrganizerInfo>) -> RpcResult<proto::Empty>
    {
        tracing::debug!("org->srv handshake received");
        self.server.organizer_has_connected.store(true, Relaxed);
        Ok(Response::new(proto::Empty {}))
    }

    async fn client_define_actions(&self, req: Request<org::ClientDefineActionsRequest>) -> RpcResult<proto::Empty>
    {
        let req = req.into_inner();
        to_rpc_empty(self.server.emit_cmd(client_cmd!(DefineActions, {actions: req.actions}), SendTo::UserSession(&req.sid)))
    }

    async fn client_show_page(&self, req: Request<org::ClientShowPageRequest>) -> RpcResult<proto::Empty>
    {
        let req = req.into_inner();
        to_rpc_empty(self.server.emit_cmd(client_cmd!(ShowPage, {page_items: req.page_items}), SendTo::UserSession(&req.sid)))
    }

    /// Send a message to one or more user sessions.
    async fn client_show_user_message(&self, req: Request<org::ClientShowUserMessageRequest>) -> RpcResult<proto::Empty>
    {
        use org::client_show_user_message_request::Recipient;
        use crate::api_server::SendTo;

        let req = req.into_inner();
        let msg_in = match req.msg {
            Some(m) => m,
            None => return Err(Status::invalid_argument("No message specified")),
        };
        let recipient = match req.recipient {
            Some(s) => s,
            None => return Err(Status::invalid_argument("No recipient specified")),
        };

        let comment_id = match msg_in.refs.clone().and_then(|r| r.comment_id) {
            Some(c) => match c.parse::<i32>() {
                Ok(i) => Some(i),
                Err(e) => return Err(Status::invalid_argument(format!("Invalid comment ID: {}", e))),
            },
            None => None,
        };

        let send_msg = |username: &str, to: SendTo, persist: bool| -> anyhow::Result<()> {
            let msg = models::MessageInsert {
                user_id: username.to_string(),
                seen: false,
                video_id: msg_in.refs.clone().and_then(|r| r.video_id),
                comment_id: comment_id,
                event_name: proto_msg_type_to_event_name((&msg_in).r#type()).to_string(),
                message: msg_in.message.clone(),
                details: msg_in.details.clone().unwrap_or_default(),
            };
            self.server.push_notify_message(&msg, to, persist)
        };

        let res = match recipient {
            Recipient::Sid(sid) => {
                if let Some(ses) = self.server.get_session(&sid) {
                    send_msg(&ses.user_name, SendTo::UserSession(&sid), false)
                } else {
                    Err(anyhow::anyhow!("Session not found"))
                }
            },
            Recipient::UserTemp(username) => { send_msg(&username, SendTo::UserId(&username), false) },
            Recipient::UserPersist(username) => { send_msg(&username, SendTo::UserId(&username), true) },
            Recipient::VideoId(id) => { send_msg(&id, SendTo::VideoId(&id), false) },
            Recipient::CollabSession(csi) => { send_msg(&csi, SendTo::Collab(&csi), false) },
        };

        to_rpc_empty(res)
    }

    async fn client_open_video(&self, req: Request<org::ClientOpenVideoRequest>) -> RpcResult<proto::Empty>
    {
        let req = req.into_inner();
        to_rpc_empty(crate::api_server::ws_handers::send_open_video_cmd(&self.server, &req.sid, &req.id).await)
    }

    async fn client_set_cookies(&self, req: Request<org::ClientSetCookiesRequest>) -> RpcResult<proto::Empty>
    {
        let req = req.into_inner();
        rpc_expect_field(&req.cookies, "filter")?;
        to_rpc_empty(self.server.emit_cmd(client_cmd!(SetCookies, {cookies: req.cookies, expire_time: req.expire_time}), SendTo::UserSession(&req.sid)))
    }

    // ========================================================================
    // Database functions
    // ========================================================================
    // (These aggregate a lot of filtering and paging functionality into a relatively
    // few RPC calls, so there's quite a bit of matching and dense logic here.)


    async fn db_get_videos(&self, req: Request<org::DbGetVideosRequest>) -> RpcResult<org::DbVideoList>
    {
        use org::db_get_videos_request::Filter;
        use org::graph_obj_rel::Rel;
        let req = req.into_inner();
        let db = self.server.db.clone();
        let pg = req.paging.as_ref().try_into()?;
        let items = match rpc_expect_field(&req.filter, "filter")?
        {
            Filter::All(_) => { models::Video::get_all(&db, pg)? },
            Filter::Ids(ids) => { paged_vec(models::Video::get_many(&db, &ids.ids)?, pg) },
            Filter::UserId(user_id) => { models::Video::get_by_user(&db, &user_id, pg)? },
            Filter::GraphRel(rel) => {
                let et = rel.edge_type.as_ref().map(|s| s.as_str());
                let vids = match rpc_expect_field(&rel.rel, "GraphObjRel.rel")? {
                    Rel::ParentOf(id) => models::Video::graph_get_by_parent(&db, id.try_into()?, et)?,
                    Rel::ChildOf(id) => models::Video::graph_get_by_child(&db, id.try_into()?, et)?
                }.into_iter().map(|v| v.obj).collect::<Vec<_>>();
                paged_vec(vids, pg)
            },
        };
        Ok(Response::new(org::DbVideoList {
            items: items.into_iter().map(|v| v.to_proto3(&self.server.url_base)).collect(),
            paging: req.paging,
        }))
    }


    async fn db_get_comments(&self, req: Request<org::DbGetCommentsRequest>) -> RpcResult<org::DbCommentList>
    {
        use org::db_get_comments_request::Filter;
        use org::graph_obj_rel::Rel;
        let req = req.into_inner();
        let db = self.server.db.clone();
        let pg = req.paging.as_ref().try_into()?;
        let items = match rpc_expect_field(&req.filter, "filter")?
        {
            Filter::All(_) => { models::Comment::get_all(&db, pg)? },
            Filter::Ids(ids) => {
                let ids = ids.ids.iter().map(|s| s.parse::<i32>()).collect::<Result<Vec<_>, _>>()
                    .map_err(|e| Status::invalid_argument(format!("Invalid comment ID: {}", e)))?;
                paged_vec(models::Comment::get_many(&db, &ids)?, pg)
            },
            Filter::UserId(user_id) => { models::Comment::get_by_user(&db, user_id, pg)? },
            Filter::VideoId(video_id) => { models::Comment::get_by_video(&db, video_id, pg)? },
            Filter::GraphRel(rel) => {
                let et = rel.edge_type.as_ref().map(|s| s.as_str());
                let comms = match rpc_expect_field(&rel.rel, "GraphObjRel.rel")? {
                    Rel::ParentOf(id) => models::Comment::graph_get_by_parent(&db, id.try_into()?, et)?,
                    Rel::ChildOf(id) => models::Comment::graph_get_by_child(&db, id.try_into()?, et)?
                }.into_iter().map(|c| c.obj).collect::<Vec<_>>();
                paged_vec(comms, pg)
            },
        };
        Ok(Response::new(org::DbCommentList {
            items: items.into_iter().map(|c| c.to_proto3()).collect(),
            paging: req.paging,
        }))
    }


    async fn db_get_user_messages(&self, req: Request<org::DbGetUserMessagesRequest>) -> RpcResult<org::DbUserMessageList>
    {
        use org::db_get_user_messages_request::Filter;
        let req = req.into_inner();
        let db = self.server.db.clone();
        let pg = req.paging.as_ref().try_into()?;
        let items = match rpc_expect_field(&req.filter, "filter")?
        {
            Filter::All(_) => { models::Message::get_all(&db, pg)? },
            Filter::Ids(ids) => {
                let ids = ids.ids.iter().map(|s| s.parse::<i32>()).collect::<Result<Vec<_>, _>>()
                    .map_err(|e| Status::invalid_argument(format!("Invalid user message ID: {}", e)))?;
                paged_vec(models::Message::get_many(&db, &ids)?, pg)
            },
            Filter::UserId(user_id) => { models::Message::get_by_user(&db, user_id, pg)? },
            Filter::VideoId(video_id) => { models::Message::get_by_video(&db, video_id, pg)? },
            Filter::CommentId(comment_id) => {
                let comment_id = comment_id.parse::<i32>()
                    .map_err(|e| Status::invalid_argument(format!("Invalid comment ID: {}", e)))?;
                models::Message::get_by_comment(&db, comment_id)?
            },
        };
        Ok(Response::new(org::DbUserMessageList {
            items: items.into_iter().map(|m| m.to_proto3()).collect(),
            paging: req.paging,
        }))
    }


    async fn db_get_prop_nodes(&self, req: Request<org::DbGetPropNodesRequest>) -> RpcResult<org::DbPropNodeList>
    {
        use org::graph_obj_rel::Rel;

        let req = req.into_inner();
        let db = self.server.db.clone();
        let pg = req.paging.as_ref().try_into()?;

        let ids = if let Some(ids) = req.ids {
            Some(ids.ids.iter().map(|s| s.parse::<i32>()).collect::<Result<Vec<_>, _>>()
                .map_err(|e| Status::invalid_argument(format!("Invalid user message ID: {}", e)))?)
        } else { None };

        let items = match (req.node_type, ids, req.graph_rel) {
            (None, None, None) => models::PropNode::get_all(&db, pg)?,
            (None, Some(ids), None) => paged_vec(models::PropNode::get_many(&db, ids.as_slice() )?, pg),
            (Some(node_type), ids, None) => paged_vec(models::PropNode::get_by_type(&db, &node_type, &ids)?, pg),
            (node_type, ids, Some(rel)) => {
                let et = rel.edge_type.as_ref().map(|s| s.as_str());
                let objs = match rpc_expect_field(&rel.rel, "GraphObjRel.rel")? {
                    Rel::ParentOf(id) => models::PropNode::graph_get_by_parent(&db, id.try_into()?, et)?,
                    Rel::ChildOf(id) => models::PropNode::graph_get_by_child(&db, id.try_into()?, et)?
                }.into_iter().map(|c| c.obj).collect::<Vec<_>>();

                let objs = match node_type {
                    Some(node_type) => objs.into_iter().filter(|o| o.node_type == node_type).collect(),
                    None => objs,
                };
                let objs = match ids {
                    Some(ids) => objs.into_iter().filter(|o| ids.contains(&o.id)).collect(),
                    None => objs,
                };
                paged_vec(objs, pg)
            },
        };
        Ok(Response::new(org::DbPropNodeList {
            items: items.into_iter().map(|o| org::PropNode {
                id: o.id.to_string(),
                node_type: o.node_type,
                body: o.body,
            }).collect(),
            paging: req.paging,
        }))
    }


    async fn db_get_prop_edges(&self, req: Request<org::DbGetPropEdgesRequest>) -> RpcResult<org::DbPropEdgeList>
    {
        let req = req.into_inner();
        let db = self.server.db.clone();
        let pg = req.paging.as_ref().try_into()?;

        let ids = if let Some(ids) = req.ids {
            Some(ids.ids.iter().map(|s| s.parse::<i32>()).collect::<Result<Vec<_>, _>>()
                .map_err(|e| Status::invalid_argument(format!("Invalid user message ID: {}", e)))?)
        } else { None };

        let items = models::PropEdge::get_filtered(&db,
            req.from.as_ref().map(|o| o.try_into()).transpose()?,
            req.to.as_ref().map(|o| o.try_into()).transpose()?,
            req.edge_type.as_deref(),
            &ids, pg)?;

        Ok(Response::new(org::DbPropEdgeList {
            items: items.into_iter().map(|o| o.to_proto3()).collect(),
            paging: req.paging,
        }))
    }

    async fn db_upsert(&self, req: Request<org::DbUpsertRequest>) -> RpcResult<org::DbUpsertResponse>
    {
        let req = req.into_inner();
        let db = self.server.db.clone();
        macro_rules! upsert_type {
            ([$db:expr, $input_items:expr, $model:ty, $ins_model:ty, $id_missing:expr, $to_proto:expr]) => {
                {
                    let inserts = $input_items.iter().filter(|it| $id_missing(it))
                        .map(|it| <$ins_model>::from_proto3(it))
                        .collect::<Result<Vec<_>, _>>()?;

                    let updates = $input_items.iter().filter(|it| !$id_missing(it))
                        .map(|it| <$model>::from_proto3(it))
                        .collect::<Result<Vec<_>, _>>()?;

                    // Perform database operations
                    let ins_res = <$model>::insert_many($db, &inserts)?;
                    let upd_res = <$model>::update_many($db, &updates)?;

                    if ins_res.len() + upd_res.len() != $input_items.len() {
                        return Err(Status::internal("Database upsert returned unexpected number of results"));
                    }

                    // Combine the results in the original order
                    let mut ins_iter = ins_res.into_iter();
                    let mut upd_iter = upd_res.into_iter();
                    let res_comb_orig_order = $input_items.iter().map(|it| {
                        if $id_missing(it) {
                            ins_iter.next().expect("Insert result missing")
                        } else {
                            upd_iter.next().expect("Update result missing")
                        }
                    }).collect::<Vec<_>>();

                    // Convert back to proto3
                    res_comb_orig_order.iter().map(|it| $to_proto(it)).collect::<Vec<_>>()
                }
            }
        }
        Ok(Response::new(org::DbUpsertResponse {
            videos: upsert_type!([
                db.as_ref(), req.videos, models::Video, models::VideoInsert,
                |it: &proto::Video| it.id.is_empty(),
                |it: &models::Video| it.to_proto3(self.server.url_base.as_str())]),
            comments: upsert_type!([
                db.as_ref(), req.comments, models::Comment, models::CommentInsert,
                |it: &proto::Comment| it.id.is_empty(),
                |it: &models::Comment| it.to_proto3()]),
            user_messages: upsert_type!([
                db.as_ref(), req.user_messages, models::Message, models::MessageInsert,
                |it: &proto::UserMessage| it.id.is_none(),
                |it: &models::Message| it.to_proto3()]),
            nodes: upsert_type!([
                db.as_ref(), req.nodes, models::PropNode, models::PropNodeInsert,
                |it: &org::PropNode| it.id.is_empty(),
                |it: &models::PropNode| it.to_proto3()]),
            edges: upsert_type!([
                db.as_ref(), req.edges, models::PropEdge, models::PropEdgeInsert,
                |it: &org::PropEdge| it.id.is_empty(),
                |it: &models::PropEdge| it.to_proto3()]),
        }))
    }

    async fn db_delete(&self, req: Request<org::DbDeleteRequest>) -> RpcResult<org::DbDeleteResponse>
    {
        let req = req.into_inner();
        let db = self.server.db.clone();
        macro_rules! delete_type {
            ([$db:expr, $input_ids:expr, $id_type:ty, $model:ty]) => {
                {
                    use std::str::FromStr;
                    let ids = $input_ids.iter().map(|s| <$id_type>::from_str(&s)
                            .map_err(|e| Status::invalid_argument(format!("Invalid ID: {}", e)))
                        ).collect::<Result<Vec<_>, _>>()?;
                    <$model>::delete_many($db, ids.as_slice())? as u32
                }
            }
        }
        Ok(Response::new(org::DbDeleteResponse {
            videos_deleted: delete_type!([db.as_ref(), req.video_ids, String, models::Video]),
            comments_deleted: delete_type!([db.as_ref(), req.comment_ids, i32, models::Comment]),
            user_messages_deleted: delete_type!([db.as_ref(), req.user_message_ids, i32, models::Message]),
            nodes_deleted: delete_type!([db.as_ref(), req.node_ids, i32, models::PropNode]),
            edges_deleted: delete_type!([db.as_ref(), req.edge_ids, i32, models::PropEdge]),
        }))
    }

    async fn db_begin_transaction(&self, _req: Request<org::DbBeginTransactionRequest>) -> RpcResult<proto::Empty>
    {
        to_rpc_empty(crate::database::begin_transaction(&self.server.db.conn()?))
    }

    async fn db_commit_transaction(&self, _req: Request<org::DbCommitTransactionRequest>) -> RpcResult<proto::Empty>
    {
        to_rpc_empty(crate::database::commit_transaction(&self.server.db.conn()?))
    }

    async fn db_rollback_transaction(&self, _req: Request<org::DbRollbackTransactionRequest>) -> RpcResult<proto::Empty>
    {
        to_rpc_empty(crate::database::rollback_transaction(&self.server.db.conn()?))
    }
}


fn to_rpc_empty<T, E>(res: Result<T, E>) -> RpcResult<proto::Empty>
    where E: std::fmt::Display,
{
    match res {
        Ok(_) => Ok(Response::new(proto::Empty {})),
        Err(e) => Err(Status::internal(e.to_string())),
    }
}


pub async fn run_org_to_srv_grpc_server(bind: GrpcBindAddr, server: ServerState) -> anyhow::Result<()>
{
    let span = tracing::info_span!("gRPC server for org->srv");
    let terminate_flag = server.terminate_flag.clone();
    let service = org::organizer_outbound_server::OrganizerOutboundServer::new(OrganizerOutboundImpl {
        server,
    });
    run_grpc_server(bind, service, span, terminate_flag).await
}

pub fn make_grpc_server_bind(tcp: &Option<String>, data_dir: &Path) -> anyhow::Result<GrpcBindAddr>
{
    match tcp {
        None => Ok(GrpcBindAddr::Unix(data_dir
            .canonicalize().context("Expanding data dir")?
            .join("grpc-org-to-srv.sock").into())),
        Some(s) => Ok(GrpcBindAddr::Tcp(s.parse().context("Parsing TCP listen address")?)),
    }
}
