use std::sync::Arc;
use db_check::ErrorsPerVideo;
use folder_ops::create_folder;
use ui_components::{make_folder_list_popup_actions, construct_navi_page, construct_permission_page};

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

use crate::db_check::spawn_database_check;
use crate::folder_ops::{get_current_folder_path, FolderData};

mod folder_ops;
mod db_check;
mod ui_components;
mod graph_utils;

pub type GrpcServerConn = OrganizerOutboundClient<Channel>;

#[derive(Default)]
pub struct SimpleOrganizer {
    client: Arc<Mutex<Option<GrpcServerConn>>>,
    db_checker_res: Arc<Mutex<Option<anyhow::Result<ErrorsPerVideo>>>>,
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
        self.client.lock().await.replace(client.clone());

        spawn_database_check(Arc::new(Mutex::new(client)), self.db_checker_res.clone());
        Ok(Response::new(proto::Empty {}))
    }

    async fn navigate_page(&self, req: Request<org::NavigatePageRequest>) -> RpcResponseResult<org::ClientShowPageRequest>
    {
        let req = req.into_inner();
        let ses = proto3_get_field!(&req, ses, "No session data in request")?;
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;

        if self.check_db_setup_task().await? {
            return Ok(Response::new(org::ClientShowPageRequest {
                sid: ses.sid.clone(),
                page_items: vec![
                    proto::PageItem { item: Some(proto::page_item::Item::Html(r#"
                        <h1>Organizer database setup...</h1>
                        <p>Database check is still running, please wait...</p>
                    "#.into())) },
                ],
            }));
        }

        let page = construct_navi_page(&mut srv, &ses).await?;

        //let page = construct_permission_page(&mut srv, &ses).await?;
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
        self.check_db_setup_task().await?;
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


impl SimpleOrganizer
{
    /// Check if database check is still running.
    /// If it's done, send any error messages to clients.
    /// Returns true if the check is still running, false if it is complete.
    pub async fn check_db_setup_task(&self) -> RpcResult<bool> {
        match self.db_checker_res.lock().await.as_ref() {
            None => Ok(true), // Still running
            Some(Ok(video_errs)) =>
            {
                // Database check is complete, send any error messages to clients
                if !video_errs.is_empty() {
                    let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
                    for (video_id, err) in video_errs {
                        tracing::warn!("Sending error message to client for video '{}': {}", video_id, err);
                        srv.client_show_user_message(org::ClientShowUserMessageRequest {
                            msg: Some(proto::UserMessage {
                                message: format!("Organizer error: {}", err),
                                r#type: proto::user_message::Type::Error.into(),
                                ..Default::default()
                            }),
                            recipient: Some(org::client_show_user_message_request::Recipient::VideoId(video_id.clone())),
                            ..Default::default()
                        }).await?;
                    }
                    // Clear the error list to avoid resending them
                    self.db_checker_res.lock().await.replace(Ok(ErrorsPerVideo::new()));
                }
                Ok(false)
            },
            Some(Err(e)) => Err(Status::internal(format!("Database check failed: {:?}", e))),
        }
    }
}
