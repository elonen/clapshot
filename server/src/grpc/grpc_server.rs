use std::{sync::atomic::Ordering::Relaxed, path::Path};
use anyhow::Context;
use tonic::{Request, Response, Status};
use crate::{api_server::{server_state::ServerState}, database::models::proto_msg_type_to_event_name};
use crate::database::models;

use lib_clapshot_grpc::{proto, RpcResult, GrpcBindAddr, run_grpc_server};
use lib_clapshot_grpc::proto::org;

pub struct OrganizerOutboundImpl {
    server: ServerState,
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

// Implement the RCP methods

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
                ref_video_id: msg_in.refs.clone().and_then(|r| r.video_id),
                ref_comment_id: comment_id,
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

    async fn delete_video(&self, req: Request<org::DeleteVideoRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn modify_video(&self, req: Request<org::ModifyVideoRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
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
