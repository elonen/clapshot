use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use tonic::transport::{Endpoint, Uri, Channel};
use tokio::net::UnixStream;
use tower::service_fn;

use clapshot_server::grpc::proto::organizer_outbound_client::OrganizerOutboundClient;
use clapshot_server::grpc::proto::server_info::grpc_endpoint;
use clapshot_server::grpc::proto;

//use proto::{server_info::grpc_endpoint, organizer_outbound_client::OrganizerOutboundClient};


pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const NAME: &'static str = env!("CARGO_PKG_NAME");

/*
pub mod proto {
    tonic::include_proto!("clapshot.organizer");
    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("organizer_descriptor");
}
*/


#[derive(Debug, Default)]
pub struct SimpleOrganizer {
    client: Arc<Mutex<Option<OrganizerOutboundClient<Channel>>>>,
}
type RpcResult<T> = Result<Response<T>, Status>;


// Implement inbound RCP methods

#[tonic::async_trait]
impl proto::organizer_inbound_server::OrganizerInbound for SimpleOrganizer
{
    async fn handshake(&self, req: Request<proto::ServerInfo>) -> RpcResult<proto::Empty>
    {
        // Check version
        let my_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        let their_version = req.get_ref().version.as_ref().ok_or_else(|| {
            Status::invalid_argument("No version in request") })?;
        if my_version.major != their_version.major {
            return Err(Status::invalid_argument(format!("Major version mismatch: organizer='{}', clapshot='{:?}'", my_version, their_version)));
        }
 
        // Connect back to organizer
        let bc = req.get_ref().backchannel.as_ref().ok_or_else(|| {
            Status::invalid_argument("No backchannel in request") })?;
        let ep = bc.endpoint.as_ref().ok_or_else(|| {
            Status::invalid_argument("No backchannel endpoint in request") })?.clone();

        tracing::info!("Connecting org->srv");
        let channel = match ep {
            grpc_endpoint::Endpoint::Unix(unix) =>
            {
                let path = unix.path.clone();
                if !std::path::Path::new(&path).exists() {
                    return Err(Status::invalid_argument(format!("Unix socket does not exist: {}", path)));
                }
                Endpoint::try_from("file://dummy")
                    .map_err(|e| Status::invalid_argument(format!("Failed to parse org->srv URI: {:?}", e)))?
                    .connect_timeout(std::time::Duration::from_secs(8))
                    .connect_with_connector(service_fn(move |_: Uri| {
                        UnixStream::connect(path.clone()) })).await
                        .map_err(|e| Status::invalid_argument(format!("UnixStream::connect org->srv failed: {:?}", e)))?
            },
            grpc_endpoint::Endpoint::Tcp(tcp) =>
            {
                let url = format!("http://{}:{}", tcp.host, tcp.port);
                Channel::from_shared(url)
                    .map_err(|e| Status::invalid_argument(format!("Failed to parse org->srv HTTP URI: {:?}", e)))?
                    .connect_timeout(std::time::Duration::from_secs(8))
                    .connect().await
                    .map_err(|e| Status::invalid_argument(format!("HTTP Channel::connect org->srv failed: {:?}", e)))?
            },
        };

        let mut client = OrganizerOutboundClient::new(channel);
        client.handshake(crate::proto::OrganizerInfo {}).await?;
        self.client.lock().await.replace(client);

        Ok(Response::new(proto::Empty {}))
    }

    async fn navigate_page(&self, req: Request<proto::NavigatePageRequest>) -> RpcResult<proto::ClientShowPageRequest>
    {
        let ses = req.get_ref().ses.as_ref().ok_or_else(|| {
            tracing::error!("No session data in request");
            Status::invalid_argument("No session data in request")
        })?;

        tracing::info!("Got a request: {:?}", req);
        Ok(Response::new(proto::ClientShowPageRequest {
            items: vec![],
            sid: ses.sid.clone(),
            path: "/not_implemented".into(),
        }))
    }

    async fn authz_user_action(&self, req: Request<proto::AuthzUserActionRequest>) -> RpcResult<proto::AuthzResult>
    {
       tracing::info!("Got a request: {:?}", req);
        Ok(Response::new(proto::AuthzResult {
            is_authorized: true,
            message: Some("NOT IMPLEMENTED".into()),
            details: Some("NOT IMPLEMENTED".into()),
        }))
    }
}


/*
/// Connect to a gRPC server, either via a Unix socket or HTTP(S).
/// Plain path string means Unix socket, "http://..." or "https://..." means HTTP(S).
pub async fn connect(uri: OrganizerURI) -> anyhow::Result<OrganizerConnection> 
{
    let channel = match uri {
        OrganizerURI::UnixSocket(path) =>
        {
            unix_socket::wait_for(&path, 5.0).await?;
            Endpoint::try_from("file://dummy")?
                .connect_with_connector(service_fn(move |_: Uri| {
                    UnixStream::connect(path.clone()) })).await
                .context("UnixStream::connect failed")?
        },
        OrganizerURI::Http(uri) =>
        {
            Channel::from_shared(uri.to_string()).context("Failed to parse organizer HTTP URI")?
                .connect_timeout(std::time::Duration::from_secs(8))
                .connect().await.context("HTTP Channel::connect failed")?
        },
    };
    Ok(OrganizerInboundClient::new(channel))
}
*/