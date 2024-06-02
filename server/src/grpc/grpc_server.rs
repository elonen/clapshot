use std::{path::Path, sync::atomic::Ordering::Relaxed};
use anyhow::Context;
use tonic::{Request, Response, Status};
use crate::{api_server::{server_state::ServerState, ws_handers::del_media_file_and_cleanup, SendTo}, client_cmd, database::{DbBasicQuery, DbQueryByMediaFile, DbQueryByUser, DbUpdate}, grpc::grpc_impl_helpers::{paged_vec, rpc_expect_field}, optional_str_to_i32_or_tonic_error, str_to_i32_or_tonic_error};
use crate::grpc::db_models::proto_msg_type_to_event_name;
use crate::database::models;

use lib_clapshot_grpc::{proto::{self}, run_grpc_server, GrpcBindAddr, RpcResult};
use lib_clapshot_grpc::proto::org;

pub struct OrganizerOutboundImpl {
    server: ServerState,
}

// Implement RCP methods for Organizer -> Server

#[tonic::async_trait]
impl org::organizer_outbound_server::OrganizerOutbound for OrganizerOutboundImpl
{
    async fn handshake(&self, req: tonic::Request<org::OrganizerInfo>) -> RpcResult<proto::Empty>
    {
        tracing::debug!("org->srv handshake received");
        self.server.organizer_info.lock().await.replace(req.into_inner());
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
        to_rpc_empty(self.server.emit_cmd(client_cmd!(ShowPage, {
            page_items: req.page_items,
            page_id: req.page_id,
            page_title: req.page_title,
        }), SendTo::UserSession(&req.sid)))
    }

    /// Send a message to one or more user sessions.
    async fn client_show_user_message(&self, req: Request<org::ClientShowUserMessageRequest>) -> RpcResult<proto::Empty>
    {
        use org::client_show_user_message_request::Recipient;
        use crate::api_server::SendTo;

        let req = req.into_inner();

        let msg_in = req.msg.map_or_else(|| return Err(Status::invalid_argument("No message specified")), Ok)?;
        let recipient = req.recipient.ok_or_else(|| Status::invalid_argument("No recipient specified"))?;

        let (media_file_id, comment_id, subtitle_id) = match &msg_in.refs {
            Some(refs) => (
                refs.media_file_id.clone(),
                optional_str_to_i32_or_tonic_error!(refs.comment_id)?,
                optional_str_to_i32_or_tonic_error!(refs.subtitle_id)?
            ),
            None => (None, None, None),
        };

        let send_msg = |username: &str, to: SendTo, persist: bool| -> anyhow::Result<()> {
            let msg = models::MessageInsert {
                user_id: username.to_string(),
                seen: false,
                media_file_id,
                comment_id,
                subtitle_id,
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
            Recipient::MediaFileId(id) => { send_msg(&id, SendTo::MediaFileId(&id), false) },
            Recipient::CollabSession(csi) => { send_msg(&csi, SendTo::Collab(&csi), false) },
        };

        to_rpc_empty(res)
    }

    async fn client_open_media_file(&self, req: Request<org::ClientOpenMediaFileRequest>) -> RpcResult<proto::Empty>
    {
        let req = req.into_inner();
        to_rpc_empty(crate::api_server::ws_handers::send_open_media_file_cmd(&self.server, &req.sid, &req.id).await)
    }

    async fn client_set_cookies(&self, req: Request<org::ClientSetCookiesRequest>) -> RpcResult<proto::Empty>
    {
        let req = req.into_inner();
        to_rpc_empty(self.server.emit_cmd(client_cmd!(SetCookies, {cookies: req.cookies, expire_time: req.expire_time}), SendTo::UserSession(&req.sid)))
    }

    async fn delete_media_file(&self, req: Request<org::DeleteMediaFileRequest>) -> RpcResult<proto::Empty>
    {
        let req = req.into_inner();
        to_rpc_empty(del_media_file_and_cleanup(req.id.as_str(), None, &self.server).await)
    }

    // ========================================================================
    // Database functions
    // ========================================================================
    // (These aggregate a lot of filtering and paging functionality into a relatively
    // few RPC calls, so there's quite a bit of matching and dense logic here.)


    async fn db_get_media_files(&self, req: Request<org::DbGetMediaFilesRequest>) -> RpcResult<org::DbMediaFileList>
    {
        use org::db_get_media_files_request::Filter;
        let req = req.into_inner();
        let db = self.server.db.clone();
        let pg = req.paging.as_ref().try_into()?;
        let conn = &mut db.conn()?;
        let items = match rpc_expect_field(&req.filter, "filter")? {
            Filter::All(_) => { models::MediaFile::get_all(conn, pg)? },
            Filter::Ids(ids) => { paged_vec(models::MediaFile::get_many(conn, &ids.ids)?, pg) },
            Filter::UserId(user_id) => { models::MediaFile::get_by_user(conn, &user_id, pg)? },
        };

        let mut proto_items = Vec::with_capacity(items.len());
        for mf in items { proto_items.push(mf.to_proto3(&self.server.url_base, mf.get_subtitles(conn)?)); }

        Ok(Response::new(org::DbMediaFileList {
            items: proto_items,
            paging: req.paging,
        }))
    }


    async fn db_get_comments(&self, req: Request<org::DbGetCommentsRequest>) -> RpcResult<org::DbCommentList>
    {
        use org::db_get_comments_request::Filter;
        let req = req.into_inner();
        let db = self.server.db.clone();
        let pg = req.paging.as_ref().try_into()?;
        let conn = &mut db.conn()?;

        let items = match rpc_expect_field(&req.filter, "filter")? {
            Filter::All(_) => { models::Comment::get_all(conn, pg)? },
            Filter::Ids(ids) => {
                let ids = ids.ids.iter().map(|comment_id| str_to_i32_or_tonic_error!(comment_id)).collect::<Result<Vec<_>, _>>()?;
                paged_vec(models::Comment::get_many(conn, &ids)?, pg)
            },
            Filter::UserId(user_id) => { models::Comment::get_by_user(conn, user_id, pg)? },
            Filter::MediaFileId(media_file_id) => { models::Comment::get_by_media_file(conn, media_file_id, pg)? },
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
        let conn = &mut db.conn()?;
        let items = match rpc_expect_field(&req.filter, "filter")? {
            Filter::All(_) => { models::Message::get_all(conn, pg)? },
            Filter::Ids(ids) => {
                let ids = ids.ids.iter().map(|message_id| str_to_i32_or_tonic_error!(message_id)).collect::<Result<Vec<_>, _>>()?;
                paged_vec(models::Message::get_many(conn, ids.as_slice())?, pg)
            },
            Filter::UserId(user_id) => { models::Message::get_by_user(conn, user_id, pg)? },
            Filter::MediaFileId(media_file_id) => { models::Message::get_by_media_file(conn, media_file_id, pg)? },
            Filter::CommentId(comment_id) => { models::Message::get_by_comment(conn, str_to_i32_or_tonic_error!(comment_id)?)? },
        };
        Ok(Response::new(org::DbUserMessageList {
            items: items.into_iter().map(|m| m.to_proto3()).collect(),
            paging: req.paging,
        }))
    }


    async fn db_upsert(&self, req: Request<org::DbUpsertRequest>) -> RpcResult<org::DbUpsertResponse>
    {
        let req = req.into_inner();
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
                    res_comb_orig_order.iter().map(|it| $to_proto(it)).collect::<Result<Vec<_>, tonic::Status>>()
                }
            }
        }
        let conn = &mut self.server.db.conn()?;
        Ok(Response::new(org::DbUpsertResponse {
            media_files: upsert_type!([
                conn, req.media_files, models::MediaFile, models::MediaFileInsert,
                |it: &proto::MediaFile| it.id.is_empty(),
                |it: &models::MediaFile| Ok(it.to_proto3(self.server.url_base.as_str(), it.get_subtitles(conn)?))])?,
            comments: upsert_type!([
                conn, req.comments, models::Comment, models::CommentInsert,
                |it: &proto::Comment| it.id.is_empty(),
                |it: &models::Comment| Ok(it.to_proto3())])?,
            user_messages: upsert_type!([
                conn, req.user_messages, models::Message, models::MessageInsert,
                |it: &proto::UserMessage| it.id.is_none(),
                |it: &models::Message| Ok(it.to_proto3())])?,
            subtitles: upsert_type!([
                conn, req.subtitles, models::Subtitle, models::SubtitleInsert,
                |it: &proto::Subtitle| it.id.is_empty(),
                |it: &models::Subtitle| Ok(it.to_proto3(self.server.url_base.as_str()))])?,
        }))
    }

    async fn db_delete(&self, req: Request<org::DbDeleteRequest>) -> RpcResult<org::DbDeleteResponse>
    {
        let req = req.into_inner();
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
        let conn = &mut self.server.db.conn()?;
        Ok(Response::new(org::DbDeleteResponse {
            media_files_deleted: delete_type!([conn, req.media_file_ids, String, models::MediaFile]),
            subtitles_deleted: delete_type!([conn, req.subtitle_ids, i32, models::Subtitle]),
            comments_deleted: delete_type!([conn, req.comment_ids, i32, models::Comment]),
            user_messages_deleted: delete_type!([conn, req.user_message_ids, i32, models::Message]),
        }))
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
    let server_listening_flag = server.grpc_srv_listening_flag.clone();

    let service = org::organizer_outbound_server::OrganizerOutboundServer::new(OrganizerOutboundImpl { server });

    run_grpc_server(bind, service, span, server_listening_flag, terminate_flag).await
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
