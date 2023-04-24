use std::{sync::atomic::Ordering::Relaxed, path::Path};
use anyhow::Context;
use tonic::{Request, Response, Status};
use crate::{api_server::{server_state::ServerState}, database::{models::proto_msg_type_to_event_name, DbBasicQuery, DbQueryByUser, DbGraphQuery, DbQueryByVideo}, grpc::{db_video_to_proto3, db_comment_to_proto3, db_message_to_proto3, grpc_impl_helpers::{rpc_expect_field, paged_vec}, db_prop_edge_to_proto3}};
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
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn client_show_page(&self, req: Request<org::ClientShowPageRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
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

        match res {
            Ok(_) => Ok(Response::new(proto::Empty {})),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn client_open_video(&self, req: Request<org::ClientOpenVideoRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn client_set_cookies(&self, req: Request<org::ClientSetCookiesRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn client_open_external_url(&self, req: Request<org::ClientOpenExternalUrlRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
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
            items: items.into_iter().map(|v| db_video_to_proto3(&v, &self.server.url_base)).collect(),
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
            items: items.into_iter().map(|c| db_comment_to_proto3(&c)).collect(),
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
            items: items.into_iter().map(|m| db_message_to_proto3(&m)).collect(),
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
            items: items.into_iter().map(|o| db_prop_edge_to_proto3(&o)).collect(),
            paging: req.paging,
        }))
    }

    async fn db_upsert(&self, req: Request<org::DbUpsertRequest>) -> RpcResult<org::DbUpsertResponse>
    {
        tracing::info!("Got a db_upsert req: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn db_delete(&self, req: Request<org::DbDeleteRequest>) -> RpcResult<org::DbDeleteResponse>
    {
        tracing::info!("Got a db_delete req: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
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
