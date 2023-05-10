use std::sync::Arc;
use folder_ops::{make_folder_list_popup_actions, create_folder, construct_navi_page};
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

use crate::folder_ops::{get_current_folder_path, FolderData};

mod folder_ops;

pub type GrpcServerConn = OrganizerOutboundClient<Channel>;

#[derive(Debug, Default)]
pub struct SimpleOrganizer {
    client: Arc<Mutex<Option<GrpcServerConn>>>,
}
pub type RpcResponseResult<T> = Result<Response<T>, Status>;
pub type RpcResult<T> = Result<T, Status>;


pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const NAME: &'static str = env!("CARGO_PKG_NAME");


// Implement inbound RCP methods

#[tonic::async_trait]
impl org::organizer_inbound_server::OrganizerInbound for SimpleOrganizer
{
    async fn handshake(&self, req: Request<org::ServerInfo>) -> RpcResponseResult<proto::Empty>
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

    async fn navigate_page(&self, req: Request<org::NavigatePageRequest>) -> RpcResponseResult<org::ClientShowPageRequest>
    {
        let req = req.into_inner();
        let ses = proto3_get_field!(&req, ses, "No session data in request")?;
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;

        let page = construct_navi_page(&mut srv, &ses).await?;
        Ok(Response::new(page))
    }

    async fn authz_user_action(&self, _req: Request<org::AuthzUserActionRequest>) -> RpcResponseResult<org::AuthzResult>
    {
        Ok(Response::new(org::AuthzResult {
            is_authorized: None,
            message: Some("NOT IMPLEMENTED".into()),
            details: Some("NOT IMPLEMENTED".into()),
        }))
    }

    async fn on_start_user_session(&self, req: Request<org::OnStartUserSessionRequest>) -> RpcResponseResult<org::OnStartUserSessionResult>
    {
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let sid = req.into_inner().ses.ok_or(Status::invalid_argument("No session ID"))?.sid;

        srv.client_define_actions(org::ClientDefineActionsRequest {
                actions: make_folder_list_popup_actions(),
                sid,
            }).await?;

        Ok(Response::new(org::OnStartUserSessionResult {}))
    }

    async fn cmd_from_client(&self, req: Request<org::CmdFromClientRequest>) -> RpcResponseResult<proto::Empty>
    {
        let req = req.into_inner();
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let ses = req.ses.ok_or(Status::invalid_argument("No session ID"))?;

        match req.cmd.as_str() {
            "new_folder" => {
                // Read args from JSON
                let args = serde_json::from_str::<FolderData>(&req.args)
                    .map_err(|e| Status::invalid_argument(format!("Failed to parse args: {:?}", e)))?;

                let path = get_current_folder_path(&mut srv, &ses).await?;
                let parent_folder = path.last().cloned();

                // Create folder in transaction
                srv.db_begin_transaction(org::DbBeginTransactionRequest {}).await?;

                match create_folder(&mut srv, &ses, parent_folder, args).await {
                    Ok(_) => {
                        srv.db_commit_transaction(org::DbCommitTransactionRequest {}).await?;

                        tracing::debug!("Folder created & committed, refreshing client's page");
                        let navi_page = construct_navi_page(&mut srv, &ses).await?;
                        srv.client_show_page(navi_page).await?;

                        Ok(Response::new(proto::Empty {}))
                    },
                    Err(e) => {
                        srv.db_rollback_transaction(org::DbRollbackTransactionRequest {}).await?;
                        Err(e)
                    }
                }
            },
            _ => {
                Err(Status::invalid_argument(format!("Unknown command: {:?}", req.cmd)))
            },
        }
    }
}
