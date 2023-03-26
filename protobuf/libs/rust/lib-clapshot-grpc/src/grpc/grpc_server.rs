use std::{path::Path, time::Duration, sync::atomic::Ordering::Relaxed};

use anyhow::Context;
use async_std::path::PathBuf;
use tonic::{Request, Response, Status};
use crate::{api_server::{server_state::ServerState}};
use crate::database::models;

use super::proto;
use super::proto::organizer_outbound_server::OrganizerOutbound;


pub struct OrganizerOutboundImpl {
    server: ServerState,
}

type RpcResult<T> = Result<Response<T>, Status>;


#[derive(Debug, Clone)]
pub enum BindAddr {
    Tcp(std::net::SocketAddr),
    Unix(PathBuf),
}
impl BindAddr {
    pub fn to_uri(&self) -> String {
        match self {
            BindAddr::Tcp(s) => format!("http://{}", s),
            BindAddr::Unix(p) => p.to_string_lossy().into(),
        }
    }
}

pub fn parse_server_bind(tcp: &Option<String>, data_dir: &Path) -> anyhow::Result<BindAddr>
{
    match tcp {
        None => Ok(BindAddr::Unix(data_dir
            .canonicalize().context("Expanding data dir")?
            .join("grpc-org-to-srv.sock").into())),
        Some(s) => Ok(BindAddr::Tcp(s.parse().context("Parsing TCP listen address")?)),
    }
}
// Implement the RCP methods

#[tonic::async_trait]
impl OrganizerOutbound for OrganizerOutboundImpl
{
    async fn handshake(&self, _req: tonic::Request<proto::OrganizerInfo>) -> RpcResult<proto::Empty>
    {
        tracing::debug!("org->srv handshake received");
        self.server.organizer_has_connected.store(true, Relaxed);
        Ok(Response::new(proto::Empty {}))
    }

    async fn client_define_actions(&self, req: Request<proto::ClientDefineActionsRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn client_show_page(&self, req: Request<proto::ClientShowPageRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn client_show_user_message(&self, req: Request<proto::ClientShowUserMessageRequest>) -> RpcResult<proto::Empty>
    {
        use proto::client_show_user_message_request::Recipient;
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

        let send_msg = |username: &str, to: SendTo, persist: bool| -> anyhow::Result<()> {
            let msg = models::MessageInsert {
                user_id: username.to_string(),
                seen: false,
                ref_video_hash: msg_in.refs.clone().and_then(|r| r.video_hash),
                ref_comment_id: msg_in.refs.clone().and_then(|r| r.comment_id),
                event_name: match (&msg_in).r#type() {
                    proto::user_message::Type::Ok => "ok",
                    proto::user_message::Type::Error => "error",
                    proto::user_message::Type::Progress => "progress",
                }.to_string(),
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
            Recipient::VideoHash(vh) => { send_msg(&vh, SendTo::VideoHash(&vh), false) },
            Recipient::CollabSession(csi) => { send_msg(&csi, SendTo::Collab(&csi), false) },
        };

        match res {
            Ok(_) => Ok(Response::new(proto::Empty {})),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn client_open_video(&self, req: Request<proto::ClientOpenVideoRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn client_set_cookies(&self, req: Request<proto::ClientSetCookiesRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn client_open_external_url(&self, req: Request<proto::ClientOpenExternalUrlRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn delete_video(&self, req: Request<proto::DeleteVideoRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }

    async fn modify_video(&self, req: Request<proto::ModifyVideoRequest>) -> RpcResult<proto::Empty>
    {
        tracing::info!("Got a request: {:?}", req);
        Err(Status::unimplemented("Not implemented"))
    }
}


pub async fn run_grpc_server(bind: BindAddr, server: ServerState) -> anyhow::Result<()>
{
    let span = tracing::info_span!("gRPC server for org->srv");

    span.in_scope(|| { tracing::info!("Binding to '{}'", bind.to_uri()) });
    
    let refl = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(proto::FILE_DESCRIPTOR_SET)
        .build()?;

    let tf = server.terminate_flag.clone();
    let srv = tonic::transport::Server::builder()
        .add_service(refl)
        .add_service(proto::organizer_outbound_server::OrganizerOutboundServer::new(OrganizerOutboundImpl {
            server,
        }));

    let wait_for_shutdown = async move {
        while !tf.load(Relaxed) { tokio::time::sleep(Duration::from_millis(10)).await; }
    };

    match bind {
        BindAddr::Tcp(addr) => {
                srv.serve_with_shutdown(addr, wait_for_shutdown).await?;
        },
        BindAddr::Unix(path) => {
            if path.exists().await {
                std::fs::remove_file(&path).context("Failed to delete previous socket.")?;
            }
            srv.serve_with_incoming_shutdown(
                    tokio_stream::wrappers::UnixListenerStream::new(
                        tokio::net::UnixListener::bind(&path)?),
                    wait_for_shutdown
                ).await?;
        }
    }
    span.in_scope(|| { tracing::info!("Exiting gracefully.") });
    Ok(())
}
