use std::collections::HashMap;
use lib_clapshot_grpc::proto::Icon;
use lib_clapshot_grpc::proto::org::{UserSessionData, PropNode};

use lib_clapshot_grpc::proto::page_item::folder_listing::item::Visualization;
use lib_clapshot_grpc::{
    proto::{self, org}
};
use tonic::Status;

use crate::{GrpcServerConn, RpcResult};

/// Name of the client cookie that holds the current folder path
const PATH_COOKIE_NAME: &str = "folder_path";

/// Node type for the singleton node that represents a unique user id.
/// Singleton key holds the user id. Body is a json-encoded UserNodeData.
const USER_ID_NODE_TYPE: &str = "user_id";

/// Node type for folders
const FOLDER_NODE_TYPE: &str = "folder";

/// Edge from video/folder to folder that contains it
const PARENT_FOLDER_EDGE_TYPE: &str = "parent_folder";

/// Edge from folder to user id
const FOLDER_OWNER_EDGE_TYPE: &str = "folder_owner";



#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct UserNodeData {
    name: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct FolderData {
    name: String,
    preview_cache: Vec<proto::page_item::folder_listing::Item>
}

/// Get or create a singleton PropNode of the given type.
/// Call this inside a transaction.
async fn get_or_add_singleton_node(srv: &mut GrpcServerConn, node_type: &str, singleton_key: &str, body: Option<String>) -> RpcResult<org::PropNode>
{
    let get_res = srv.db_get_singleton_prop_node(org::DbGetSingletonPropNodeRequest {
        node_type: node_type.into(),
        singleton_key: singleton_key.into(),
    }).await?.into_inner();

    if let Some(node) = get_res.node {
        return Ok(node);
    }

    let ins_res = srv.db_upsert(org::DbUpsertRequest {
            nodes: vec![org::PropNode {
                id: "".into(),  // empty = insert
                body,
                node_type: node_type.into(),
                singleton_key: Some(singleton_key.into()),
            }],
            ..Default::default()
        }).await?.into_inner();

    return Ok(ins_res.nodes.first()
        .ok_or(Status::internal("BUG: No node returned from insert"))?.clone())
}



/// Make sure a PropNode exists for the current user, and return it.
///
/// Call this inside a transaction.
pub (crate) async fn upsert_session_user(srv: &mut GrpcServerConn, ses: &UserSessionData)
    -> RpcResult<org::PropNode>
{
    let user = ses.user.clone().ok_or(Status::internal("No user in session"))?;
    let user_node = get_or_add_singleton_node(
        srv,
        USER_ID_NODE_TYPE,
        &user.id,
        Some( serde_json::to_string(&UserNodeData { name: user.name.clone()}).unwrap())
    ).await?;
    Ok(user_node)
}


/// Return a PropNode that represents the user that owns the given folder.
pub (crate) async fn get_folder_owner(srv: &mut GrpcServerConn, folder: &PropNode)
    -> RpcResult<PropNode>
{
    // Owner = PropNode of type USER_ID_NODE_TYPE that is pointed from
    // the folder node via FOLDER_OWNER_EDGE_TYPE
    let res = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
        graph_rel: Some(org::GraphObjRel {
            rel: Some(org::graph_obj_rel::Rel::ParentOf(
                org::GraphObj { id: Some(org::graph_obj::Id::NodeId(folder.id.clone())) })),
            edge_type: Some(FOLDER_OWNER_EDGE_TYPE.into()),
        }),
        ..Default::default()
    }).await?.into_inner();

    if res.items.len() > 1 {
        return Err(Status::internal(format!("DB consistency: folder #{} has {} (>1) owners", folder.id, res.items.len())))
    }
    if let Some(owner) = res.items.first() {
        if owner.node_type != USER_ID_NODE_TYPE {
            return Err(Status::internal(format!("DB consistency: folder #{} has owner of type '{}' (!= '{}')", folder.id, owner.node_type, USER_ID_NODE_TYPE)));
        }
        Ok(owner.clone())
    } else {
        Err(Status::internal(format!("DB consistency: folder #{} has no owner", folder.id)))
    }
}


/// Get current folder path from cookies.
/// Returns empty vector if no folder path is set.
pub (crate) async fn get_current_folder_path(srv: &mut GrpcServerConn, ses: &UserSessionData)
    -> RpcResult<Vec<org::PropNode>>
{
    let mut folder_path = vec![];
    if let Some(ck) = &ses.cookies {
        if let Some(fp_ids_json) = ck.cookies.get(PATH_COOKIE_NAME) {
            match serde_json::from_str::<Vec<String>>(fp_ids_json) {
                Ok(fp_ids) =>
                {
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
                                        vec![(PATH_COOKIE_NAME.into(), "".into())].into_iter() // empty value = remove cookie
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
                        folder_path = fp_ids.into_iter().map(|id| {
                            folders.items.iter().find(|n| n.id == id).cloned()
                                .unwrap_or_else(|| org::PropNode { id: id.clone(), body: Some(serde_json::json!({ "name": "Unknown folder" }).to_string()), ..Default::default() })
                        }).collect();
                    }
                },
                Err(e) => {
                    tracing::error!("Failed to parse folder_path cookie: {:?}", e);
                },
            }
        }
    }
    Ok(folder_path)
}



pub async fn create_folder(srv: &mut GrpcServerConn, ses: &org::UserSessionData, parent_folder: Option<org::PropNode>, args: FolderData)
    -> RpcResult<org::PropNode>
{
    use org::graph_obj::Id::NodeId;

    // Create folder node
    let new_folder = srv.db_upsert(
        org::DbUpsertRequest {
            nodes: vec![ org::PropNode {
                id: "".into(),
                body: Some(serde_json::to_string(&args).unwrap()),
                node_type: FOLDER_NODE_TYPE.into(),
                singleton_key: None
            }],
            ..Default::default()
        }).await?.into_inner().nodes.first().cloned().ok_or(Status::internal("Failed to create folder node"))?;

    // Link folder to (possible) parent
    if let Some(parent_folder) = parent_folder.clone() {
        srv.db_upsert(org::DbUpsertRequest {
            edges: vec![
                org::PropEdge {
                    id: "".into(),
                    body: None,
                    edge_type: PARENT_FOLDER_EDGE_TYPE.into(),
                    from: Some(org::GraphObj { id: Some(NodeId(new_folder.id.clone())) }),
                    to: Some(org::GraphObj { id: Some(NodeId(parent_folder.id.clone())) }),
                    ..Default::default()
                }
            ],
            ..Default::default()
        }).await?;
    }

    // Link folder to user (either parent folder owner or session user if no parent folder)
    let owner_node = match parent_folder.clone() {
        Some(parent) => get_folder_owner(srv, &parent).await?,
        None => upsert_session_user(srv, &ses).await?
    };
    srv.db_upsert(org::DbUpsertRequest {
        edges: vec![
            org::PropEdge {
                id: "".into(),
                body: None,
                edge_type: FOLDER_OWNER_EDGE_TYPE.into(),
                from: Some(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(new_folder.id.clone())) }),
                to: Some(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(owner_node.id.clone())) }),
                ..Default::default()
            }
        ],
        ..Default::default()
    }).await?;

    Ok(new_folder)
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



/// Query database for subfolders and videos in given folder. Returns (subfolders, videos).
pub async fn fetch_folder_contents(srv: &mut GrpcServerConn, folder: &org::PropNode)
    -> RpcResult<(Vec<org::PropNode>, Vec<proto::Video>)>
{
    let sub_folders_res = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
        graph_rel: Some(org::GraphObjRel {
            rel: Some(org::graph_obj_rel::Rel::ChildOf(
                org::GraphObj { id: Some(org::graph_obj::Id::NodeId(folder.id.clone())) })),
            edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
        }),
        ..Default::default()
    }).await?.into_inner();

    let videos_res = srv.db_get_videos(org::DbGetVideosRequest {
        filter: Some(org::db_get_videos_request::Filter::GraphRel(
            org::GraphObjRel {
                rel: Some(org::graph_obj_rel::Rel::ChildOf(
                    org::GraphObj { id: Some(org::graph_obj::Id::NodeId(folder.id.clone())) })),
                edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
            })),
        ..Default::default()
    }).await?.into_inner();

    Ok((sub_folders_res.items, videos_res.items))
}


/// Helper: convert a folder node to a page item.
fn folder_to_page_item(folder: &org::PropNode) -> proto::page_item::folder_listing::Item {
    let folder_data = serde_json::from_str::<FolderData>(&folder.body.clone().unwrap_or("{}".into())).unwrap_or_default();
    let f = proto::page_item::folder_listing::Folder {
        id: folder.id.clone(),
        title: if folder_data.name.is_empty() { "UNNAMED".into() } else { folder_data.name.clone() },
        preview_items: folder_data.preview_cache,
    };
    proto::page_item::folder_listing::Item {
        item: Some(proto::page_item::folder_listing::item::Item::Folder(f.clone())),
        open_action: Some(proto::ScriptCall {
            lang: proto::script_call::Lang::Javascript.into(),
            code: r#"await call_server("open_folder", {id: f.id});"#.into()
        }),
        ..Default::default()
    }
}


/// Build folder view page.
/// Reads folder_path cookie and builds a list of folders and videos in the folder.
pub async fn construct_navi_page(srv: &mut GrpcServerConn, ses: &UserSessionData)
    -> RpcResult<org::ClientShowPageRequest>
{
    let folder_path = get_current_folder_path(srv, &ses).await?;

    let breadcrumbs_html = folder_path.iter().map(|f| {
        format!(r##"<a href="#" onclick="clapshot.navigatePage({{id: '{}'}}); return false;">{}</a>"##, f.id, f.body.clone().unwrap_or("UNNAMED".into()))
    }).collect::<Vec<_>>().join(" &gt; ");

    let (folder_items, videos) = if !folder_path.is_empty()
    {
        let (folders, videos) = fetch_folder_contents(srv, &folder_path.last().unwrap()).await?;
        let folder_items = folders.into_iter().map(|f| folder_to_page_item(&f)).collect::<Vec<_>>();
        (folder_items, videos)
    }
    else
    {
        // Show videos without an explicit parent folder in the root
        let orphan_videos = srv.db_get_videos(org::DbGetVideosRequest {
                filter: Some(org::db_get_videos_request::Filter::GraphRel(
                    org::GraphObjRel {
                        rel: Some(org::graph_obj_rel::Rel::Parentless(proto::Empty {})),
                        edge_type: Some("parent_folder".into()),
                    })),
                ..Default::default()
            }).await?.into_inner().items;

        // TODO: show projects as folders in the root
        let project_folders_placeholder = vec![
            proto::page_item::folder_listing::Item {
                item: Some(proto::page_item::folder_listing::item::Item::Folder(
                    proto::page_item::folder_listing::Folder {
                        id: "project:PLACEHOLDER_TEST_PROJECT_ID".into(),
                        title: "Placeholder Test Project".into(),
                        preview_items: vec![],
                })),
                popup_actions: vec!(),
                open_action: Some(proto::ScriptCall {
                    lang: proto::script_call::Lang::Javascript.into(),
                    code: r#"await call_server("open_folder", {id: f.id});"#.into()
                }),
                vis: Some(Visualization {
                    base_color: None,
                    icon:Some( Icon {
                        src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                            classes: "fa fa-film".into(),
                            color: None,
                        })),
                        size: None,
                    })
                })
            }
        ];
        (project_folders_placeholder, orphan_videos)
    };


    let video_page_items: Vec<proto::page_item::folder_listing::Item> = videos.iter().map(|v| {
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

    let items = folder_items.into_iter().chain(video_page_items.into_iter()).collect();

    Ok(org::ClientShowPageRequest {
        sid: ses.sid.clone(),
        page_items: vec![
            proto::PageItem { item: Some(proto::page_item::Item::Html(breadcrumbs_html)) },
            proto::PageItem { item: Some(proto::page_item::Item::FolderListing( proto::page_item::FolderListing {
                items,
                allow_reordering: true,
                popup_actions: vec!["new_folder".into()],
            }
            ))},
        ],
    })
}
