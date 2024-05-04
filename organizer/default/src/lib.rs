use std::collections::HashMap;
use std::sync::Arc;
use db_check::ErrorsPerVideo;
use folder_ops::create_folder;
use lib_clapshot_grpc::proto::org::{GraphObj, OrganizerInfo, SemanticVersionNumber};
use srv_short::TransactionGuard;
use ui_components::{make_custom_actions_map, construct_navi_page, OpenFolderArgs};

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
use crate::folder_ops::{get_current_folder_path, FoldeBodyData};
use crate::graph_utils::PARENT_FOLDER_EDGE_TYPE;
use crate::srv_short::map_ids_folderitem_to_graphobj;

mod folder_ops;
mod db_check;
mod ui_components;
mod graph_utils;
mod srv_short;

pub type GrpcServerConn = OrganizerOutboundClient<Channel>;

#[derive(Default)]
pub struct DefaultOrganizer {
    client: Arc<Mutex<Option<GrpcServerConn>>>,
    db_checker_res: Arc<Mutex<Option<anyhow::Result<ErrorsPerVideo>>>>,
}
pub type RpcResponseResult<T> = Result<Response<T>, Status>;
pub type RpcResult<T> = Result<T, Status>;


pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const NAME: &'static str = env!("CARGO_PKG_NAME");


// Implement inbound RCP methods (from organizer.proto)

#[tonic::async_trait]
impl org::organizer_inbound_server::OrganizerInbound for DefaultOrganizer
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
        let client = connect_back_and_finish_handshake(&req, OrganizerInfo {
            version: Some(SemanticVersionNumber { major: my_ver.major, minor: my_ver.minor, patch: my_ver.patch }),
            name: "clapshot.organizer.default".into(),
            description: "Default (in-progress) Organizer plugin".into(),
            hard_dependencies: [].into()
        }).await?;
        self.client.lock().await.replace(client.clone());

        spawn_database_check(Arc::new(Mutex::new(client)), self.db_checker_res.clone());
        Ok(Response::new(proto::Empty {}))
    }

    // rpc check_migrations(CheckMigrationsRequest) returns (CheckMigrationsResponse); // This is called on startup, after handshake
    // rpc apply_migrations(ApplyMigrationsRequest) returns (ApplyMigrationsResponse); // Called if check_migrations returns any pending migrations

    async fn check_migrations(&self, _req: Request<org::CheckMigrationsRequest>) -> RpcResponseResult<org::CheckMigrationsResponse>
    {
        Ok(Response::new(org::CheckMigrationsResponse {
            current_schema_ver: "<TODO!>".into(),
            pending_migrations: vec![],
        }))
    }

    async fn apply_migration(&self, _req: Request<org::ApplyMigrationRequest>) -> RpcResponseResult<org::ApplyMigrationResponse>
    {
        assert!(false, "apply_migration not implemented");
        Ok(Response::new(org::ApplyMigrationResponse {}))
    }

    async fn after_migrations(&self, _req: Request<org::AfterMigrationsRequest>) -> RpcResponseResult<proto::Empty>
    {
        assert!(false, "after_migrations (dummy)");
        Ok(Response::new(proto::Empty {}))
    }


    async fn navigate_page(&self, req: Request<org::NavigatePageRequest>) -> RpcResponseResult<org::ClientShowPageRequest>
    {
        let req = req.into_inner();
        let ses = proto3_get_field!(&req, ses, "No session data in request")?;
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;

        // Return please wait page if database check is still running
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

        let page = construct_navi_page(&mut srv, &ses, None).await?;
        Ok(Response::new(page))
    }

    async fn authz_user_action(&self, _req: Request<org::AuthzUserActionRequest>) -> RpcResponseResult<org::AuthzResponse>
    {
        Ok(Response::new(org::AuthzResponse {
            is_authorized: None,
            message: Some("NOT IMPLEMENTED".into()),
            details: Some("NOT IMPLEMENTED".into()),
        }))
    }

    async fn on_start_user_session(&self, req: Request<org::OnStartUserSessionRequest>) -> RpcResponseResult<org::OnStartUserSessionResponse>
    {
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let sid = req.into_inner().ses.ok_or(Status::invalid_argument("No session ID"))?.sid;
        srv.client_define_actions(org::ClientDefineActionsRequest {
            actions: make_custom_actions_map(),
            sid }).await?;
        Ok(Response::new(org::OnStartUserSessionResponse {}))
    }

    async fn cmd_from_client(&self, req: Request<org::CmdFromClientRequest>) -> RpcResponseResult<proto::Empty>
    {
        self.check_db_setup_task().await?;
        let req = req.into_inner();
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let ses = req.ses.ok_or(Status::invalid_argument("No session ID"))?;

        match req.cmd.as_str() {
            "new_folder" =>
            {
                let args = serde_json::from_str::<FoldeBodyData>(&req.args)
                    .map_err(|e| Status::invalid_argument(format!("Failed to parse args: {:?}", e)))?;

                let parent_folder = get_current_folder_path(&mut srv, &ses, None).await?.last().cloned();

                // Create folder (in transaction)
                let tx = TransactionGuard::begin(&mut srv, "new_folder").await?;
                if let Err(e) = create_folder(&mut srv, &ses, parent_folder, args).await {
                    Err(e)  // => rollback
                } else {
                    tx.commit().await?;
                    tracing::debug!("Folder created & committed, refreshing client's page");
                    let navi_page = construct_navi_page(&mut srv, &ses, None).await?;
                    srv.client_show_page(navi_page).await?;
                    Ok(Response::new(proto::Empty {}))
                }
            },
            "open_folder" =>
            {
                let folder_to_open = serde_json::from_str::<OpenFolderArgs>(&req.args)
                    .map_err(|e| Status::invalid_argument(format!("Invalid OpenFolderArgs: {:?}", e)))?;
                let mut cwd: Vec<String> = get_current_folder_path(&mut srv, &ses, None).await?.iter().map(|f| f.id.clone()).collect();

                // If given folder ID is in cwd, remove all folders after it; otherwise, append it
                if let Some(idx) = cwd.iter().position(|fid| *fid == folder_to_open.id) {
                    cwd.truncate(idx + 1);
                } else {
                    cwd.push(folder_to_open.id.clone());
                }

                // Update folder path cookie
                let new_cookie = serde_json::to_string(&cwd).unwrap();
                tracing::debug!("Setting new folder_path cookie: {}", new_cookie);

                srv.client_set_cookies(org::ClientSetCookiesRequest {
                        cookies: HashMap::from([(crate::graph_utils::PATH_COOKIE_NAME.into(), new_cookie.clone())]),
                        sid: ses.sid.clone(),
                        expire_time: None
                    }).await?;

                // Update page to view the opened folder
                let page = construct_navi_page(&mut srv, &ses, Some(new_cookie)).await?;
                srv.client_show_page(page).await?;

                Ok(Response::new(proto::Empty {}))
            },

            _ => {
                Err(Status::invalid_argument(format!("Unknown organizer command: {:?}", req.cmd)))
            },
        }
    }


    async fn move_to_folder(&self, req: Request<org::MoveToFolderRequest>) -> RpcResponseResult<proto::Empty>
    {
        let req = req.into_inner();
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let tx = TransactionGuard::begin(&mut srv, "move_to_folder").await?;

        let dst_folder = Some(GraphObj { id: Some(org::graph_obj::Id::NodeId( req.dst_folder_id  )), ..Default::default() });
        let (mut to_delete, mut to_add) = (vec![], vec![]);
        for id in map_ids_folderitem_to_graphobj(&req.ids)? {
            to_delete.extend(
                srv.db_get_prop_edges(org::DbGetPropEdgesRequest {
                        edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
                        from: Some(GraphObj { id: Some(id.clone()), ..Default::default() }),
                        ..Default::default()
                    }).await?.into_inner().items.into_iter().map(|e| e.id));
            to_add.push(org::PropEdge {
                    edge_type: PARENT_FOLDER_EDGE_TYPE.into(),
                    from: Some(GraphObj { id: Some(id), ..Default::default() }),
                    to: dst_folder.clone(),
                    ..Default::default()
                });
        }

        srv.db_delete(org::DbDeleteRequest { edge_ids: to_delete, ..Default::default() }).await?;
        srv.db_upsert(org::DbUpsertRequest { edges: to_add, ..Default::default() }).await?;
        tx.commit().await?;

        let ses = req.ses.ok_or(Status::invalid_argument("No session ID"))?;
        let navi_page = construct_navi_page(&mut srv, &ses, None).await?;
        srv.client_show_page(navi_page).await?;

        Ok(Response::new(proto::Empty {}))
    }


    async fn reorder_items(&self, req: Request<org::ReorderItemsRequest>) -> RpcResponseResult<proto::Empty>
    {
        let req = req.into_inner();
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let tx = TransactionGuard::begin(&mut srv, "move_to_folder").await?;

        // Get edges to folder
        let folder_id = req.listing_data.get("folder_id").ok_or(Status::invalid_argument("No folder ID in listing, cannot reorder"))?.clone();
        let mut edges = srv.db_get_prop_edges(org::DbGetPropEdgesRequest {
                edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
                to: Some(GraphObj { id: Some(org::graph_obj::Id::NodeId(folder_id)), ..Default::default() }),
                ..Default::default()
            }).await?.into_inner().items;

        // Assign new sort_order values to them
        let len = req.ids.len() as f32;
        for (i, id) in map_ids_folderitem_to_graphobj(&req.ids)?.iter().enumerate() {
            if let Some(e) = edges.iter_mut().find(|e| e.from.as_ref().map(|x| &x.id) == Some(&Some(id.clone())) ) {
                e.sort_order = Some(i as f32 / len);
            }
        }

        srv.db_upsert(org::DbUpsertRequest { edges, ..Default::default() }).await?;
        tx.commit().await?;

        let ses = req.ses.ok_or(Status::invalid_argument("No session ID"))?;
        let navi_page = construct_navi_page(&mut srv, &ses, None).await?;
        srv.client_show_page(navi_page).await?;

        Ok(Response::new(proto::Empty {}))
    }


    // ------------------------------------------------------------------
    // Unit / integration tests
    // ------------------------------------------------------------------

    async fn list_tests(&self, _req: Request<proto::Empty>) -> RpcResponseResult<org::ListTestsResponse>
    {
        Ok(Response::new(org::ListTestsResponse {
            test_names: vec!["test_video_owners".into(), "test2".into()],
        }))
    }

    async fn run_test(&self, req: Request<org::RunTestRequest>) -> RpcResponseResult<org::RunTestResponse>
    {
        let req = req.into_inner();
        let test_name = req.test_name.clone();
        let span = tracing::info_span!("run_test", test_name = test_name.as_str());

        span.in_scope(|| tracing::info!("Running organizer test '{}'", test_name));

        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;

        // Wait until database check task is done
        let wait_start = chrono::Utc::now();
        while self.check_db_setup_task().await? {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            if chrono::Utc::now() - wait_start > chrono::Duration::seconds(10) {
                return Err(Status::deadline_exceeded(format!("Database check timed out.")));
            }
        }
        tracing::info!("Database check done, running test");
        match test_name.as_str() {
            "test_video_owners" => {
                let res = db_check::assert_db_check_postconds(&mut srv, span.clone()).await;
                match res {
                    Ok(_) => {
                        Ok(Response::new(org::RunTestResponse { output: "OK".into(), error: None }))
                    },
                    Err(e) => {
                        Ok(Response::new(org::RunTestResponse { output: "FAIL".into(), error: Some(format!("assert_db_check_postconds FAILED: {:?}", e)) }))
                    }
                }
            },
            "test2" => {
                Ok(Response::new(org::RunTestResponse {
                    output: "Test 2 output".into(),
                    error: None,
                }))
            },
            _ => {
                Err(Status::invalid_argument(format!("Unknown test: {:?}", req.test_name)))
            },
        }
    }
}


impl DefaultOrganizer
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
