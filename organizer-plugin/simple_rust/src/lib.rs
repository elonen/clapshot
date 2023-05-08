use std::collections::HashMap;
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
        let req = req.into_inner();
        let ses = proto3_get_field!(&req, ses, "No session data in request")?;

        let mut folder_path = vec![];
        if let Some(ck) = &ses.cookies {
            if let Some(fp_ids_json) = ck.cookies.get("folder_path") {
                match serde_json::from_str::<Vec<String>>(fp_ids_json) {
                    Ok(fp_ids) => {
                        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
                        let folders = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
                                node_type: Some("folder".into()),
                                ids: Some(org::IdList { ids: fp_ids.clone() }),
                                ..Default::default()
                            }).await?.into_inner();
                        if folders.items.len() != (&fp_ids).len()
                        {
                            // Clear cookie
                            srv.client_set_cookies(org::ClientSetCookiesRequest {
                                    cookies: Some(proto::Cookies {
                                        cookies: HashMap::from_iter(
                                            vec![("folder_path".into(), "".into())].into_iter() // empty value = remove cookie
                                        ),
                                        ..Default::default()
                                    }),
                                    sid: ses.sid.clone(),
                                    ..Default::default()
                                }).await?;

                            // Send warning to user
                            srv.client_show_user_message(org::ClientShowUserMessageRequest {
                                msg: Some(proto::UserMessage {
                                    message: "Some unknown folder IDs in folder_path cookie. Clearing it.".into(),
                                    user_id: ses.user.as_ref().map(|u| u.id.clone()),
                                    r#type: proto::user_message::Type::Error.into(),
                                    ..Default::default()
                                }),
                                recipient: Some(org::client_show_user_message_request::Recipient::Sid(ses.sid.clone())),
                                ..Default::default()
                            }).await?;

                        } else {
                            folder_path = folders.items;
                        }
                    },
                    Err(e) => {
                        tracing::error!("Failed to parse folder_path cookie: {:?}", e);
                    },
                }
            }
        }

        let bread_crumbs_html = folder_path.iter().map(|f| {
            format!(r##"<a href="#" onclick="clapshot.navigatePage({{id: '{}'}}); return false;">{}</a>"##, f.id, f.body.clone().unwrap_or("UNNAMED".into()))
        }).collect::<Vec<_>>().join(" &gt; ");



        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let videos = srv.db_get_videos(org::DbGetVideosRequest {
                filter: Some(org::db_get_videos_request::Filter::GraphRel(
                    org::GraphObjRel {
                        rel: Some(org::graph_obj_rel::Rel::Parentless(proto::Empty {})),
                        edge_type: Some("folder".into()),
                    })),
                ..Default::default()
            }).await?.into_inner();

        let videos: Vec<proto::page_item::folder_listing::Item> = videos.items.iter().map(|v| {
            proto::page_item::folder_listing::Item {
                item: Some(proto::page_item::folder_listing::item::Item::Video(v.clone())),
                open_action: Some(proto::ScriptCall {
                    lang: proto::script_call::Lang::Javascript.into(),
                    code: r#"await call_server("open_video", {id: items[0].video.id});"#.into()
                }),
                popup_actions: vec!["popup_rename".into(), "popup_trash".into()],
                vis: None,
            }
        }).collect();


        tracing::info!("Got a request: {:?}", req);
        Ok(Response::new(org::ClientShowPageRequest {
            sid: ses.sid.clone(),
            page_items: vec![
                proto::PageItem { item: Some(proto::page_item::Item::Html(bread_crumbs_html)) },
                proto::PageItem { item: Some(proto::page_item::Item::FolderListing(
                    proto::page_item::FolderListing {
                        items:videos,
                        allow_reordering: true,
                        popup_actions: vec!["new_folder".into()],
                    }
                    ))},
            ],
        }))
    }

    async fn authz_user_action(&self, req: Request<org::AuthzUserActionRequest>) -> RpcResult<org::AuthzResult>
    {
        Ok(Response::new(org::AuthzResult {
            is_authorized: None,
            message: Some("NOT IMPLEMENTED".into()),
            details: Some("NOT IMPLEMENTED".into()),
        }))
    }

    async fn on_start_user_session(&self, req: Request<org::OnStartUserSessionRequest>) -> RpcResult<org::OnStartUserSessionResult>
    {
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let sid = req.into_inner().ses.ok_or(Status::invalid_argument("No session ID"))?.sid;

        srv.client_define_actions(org::ClientDefineActionsRequest {
                actions: make_folder_list_popup_actions(),
                sid,
            }).await?;

        Ok(Response::new(org::OnStartUserSessionResult {}))
    }

    async fn cmd_from_client(&self, req: Request<org::CmdFromClientRequest>) -> RpcResult<proto::Empty>
    {
        let req = req.into_inner();
        let mut srv = self.client.lock().await.clone().ok_or(Status::internal("No server connection"))?;
        let sid = req.ses.ok_or(Status::invalid_argument("No session ID"))?.sid;

        match req.cmd.as_str() {
            "new_folder" => {
                #[derive(serde::Deserialize)] struct Arg { name: String, }
                let args = serde_json::from_str::<Arg>(&req.args)
                    .map_err(|e| Status::invalid_argument(format!("Failed to parse args: {:?}", e)))?;

                println!("&&&&&&&&&&&&&& Got new folder name: {:?}", args.name);
            },
            _ => {
                tracing::error!("Unknown command: {:?}", req.cmd);
            },
        }

        Ok(Response::new(proto::Empty {}))
    }
}



pub (crate) fn make_folder_list_popup_actions() -> HashMap<String, proto::ActionDef> {
    HashMap::from([
        ("new_folder".into(), make_new_folder_action()),
    ])
}

fn make_new_folder_action() -> proto::ActionDef {
    proto::ActionDef  {
        ui_props: Some(proto::ActionUiProps {
            label: Some(format!("New folder")),
            icon: Some(proto::Icon {
                src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                    classes: "fa fa-folder-plus".into(), color: None, })),
                ..Default::default()
            }),
            key_shortcut: None,
            natural_desc: Some(format!("Create a new folder")),
            ..Default::default()
        }),
        action: Some(proto::ScriptCall {
            lang: proto::script_call::Lang::Javascript.into(),
            code: r#"
var folder_name = (await prompt("Name for the new folder", ""))?.trim();
if (folder_name) {
    await call_organizer("new_folder", {name: folder_name});
}
                "#.into()
        })
    }
}
