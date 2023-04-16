use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use tonic::transport::Channel;

use lib_clapshot_grpc::{
    connect_back_and_finish_handshake,
    proto3_get_field,
    proto::{
        self,
        org,
        org::organizer_outbound_client::OrganizerOutboundClient,
    }
};

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const NAME: &'static str = env!("CARGO_PKG_NAME");


#[derive(Debug, Default)]
pub struct SimpleOrganizer {
    client: Arc<Mutex<Option<OrganizerOutboundClient<Channel>>>>,
}
type RpcResult<T> = Result<Response<T>, Status>;


// Implement inbound RCP methods

#[tonic::async_trait]
impl org::organizer_inbound_server::OrganizerInbound for SimpleOrganizer
{
    async fn handshake(&self, req: Request<org::ServerInfo>) -> RpcResult<proto::Empty>
    {
        // Check version
        let my_ver = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        let server_ver = proto3_get_field!(req.get_ref(), version, "No version in request")?;
        if my_ver.major != server_ver.major {
            return Err(Status::invalid_argument(format!("Major version mismatch: organizer='{}', clapshot='{:?}'", my_ver, server_ver)));
        }

        tracing::info!("Connecting back, org->srv");
        let client = connect_back_and_finish_handshake(&req).await?;
        self.client.lock().await.replace(client);

        Ok(Response::new(proto::Empty {}))
    }

    async fn navigate_page(&self, req: Request<org::NavigatePageRequest>) -> RpcResult<org::ClientShowPageRequest>
    {
        let ses = proto3_get_field!(req.get_ref(), ses, "No session data in request")?;

        tracing::info!("Got a request: {:?}", req);
        Ok(Response::new(org::ClientShowPageRequest {
            items: vec![],
            sid: ses.sid.clone(),
            path: "/not_implemented".into(),
        }))
    }

    async fn authz_user_action(&self, req: Request<org::AuthzUserActionRequest>) -> RpcResult<org::AuthzResult>
    {
       tracing::info!("Got a request: {:?}", req);
        Ok(Response::new(org::AuthzResult {
            is_authorized: None,
            message: Some("NOT IMPLEMENTED".into()),
            details: Some("NOT IMPLEMENTED".into()),
        }))
    }

    async fn on_start_user_session(&self, req: Request<org::OnStartUserSessionRequest>) -> RpcResult<org::OnStartUserSessionResult>
    {
        tracing::debug!("on_start_user_session: {:?}", req);
        Ok(Response::new(org::OnStartUserSessionResult {
            dont_send_default_actions: false,
        }))
    }
}
