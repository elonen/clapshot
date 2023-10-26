use std::collections::HashMap;
use lib_clapshot_grpc::proto::org::{UserSessionData, GraphObj};

use lib_clapshot_grpc::proto::{self, org};
use tonic::Status;

use crate::graph_utils::{PATH_COOKIE_NAME, FOLDER_NODE_TYPE, PARENT_FOLDER_EDGE_TYPE};
use org::graph_obj_rel::Rel::ParentIs;
use crate::srv_short::{TransactionGuard, mkget_user_root_folder};
use crate::{GrpcServerConn, RpcResult};



#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct UserNodeData {
    pub(crate) name: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct FoldeBodyData {
    pub name: String,

    //#[serde(default)]
    //pub preview_cache: Option<Vec<proto::page_item::folder_listing::Item>>
}


/// Get current folder path from cookies.
///
/// If the cookie is malformed, it will be replaced with an empty one.
/// Returned list will always contain at least one item (root folder).
///
/// If cookie_override is Some, it will be used instead of the cookie from session.
pub (crate) async fn get_current_folder_path(srv: &mut GrpcServerConn, ses: &UserSessionData, cookie_override: Option<String>)
    -> RpcResult<Vec<org::PropNode>>
{
    let user = ses.user.clone().ok_or(Status::internal("No user in session"))?;
    let root_folder = mkget_user_root_folder(srv, &user).await?;

    let mut folder_path = vec![];
    let cookie = if cookie_override.is_some() { cookie_override } else { ses.cookies.get(PATH_COOKIE_NAME).cloned() };

    if let Some(fp_ids_json) = cookie {
        match serde_json::from_str::<Vec<String>>(fp_ids_json.as_str()) {
            Ok(fp_ids) =>
            {
                // Get PropNodes for the folder IDs
                let folders = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
                        node_type: Some(FOLDER_NODE_TYPE.into()),
                        ids: Some(org::IdList { ids: fp_ids.clone() }),
                        ..Default::default()
                    }).await?.into_inner();

                if folders.items.len() == (&fp_ids).len()
                {
                    // Ok, all folders found
                    folder_path = fp_ids.into_iter().map(|id| {
                        folders.items.iter().find(|n| n.id == id).cloned()
                            .unwrap_or_else(|| org::PropNode { id: id.clone(), body: Some(serde_json::json!({ "name": "Unknown folder" }).to_string()), ..Default::default() })
                    }).collect();
                } else {
                    // Some folder weren't found in DB. Clear cookie...
                    srv.client_set_cookies(org::ClientSetCookiesRequest {
                            cookies: HashMap::from([ (PATH_COOKIE_NAME.into(), "".into() )]),
                            sid: ses.sid.clone(),
                            ..Default::default()
                        }).await?;
                    // ...and send warning to user:
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
                }
            },
            Err(e) => {
                tracing::error!("Failed to parse folder_path cookie: {:?}", e);
            },
        }
    }

    // Make sure root folder is first in the path
    if root_folder.id != folder_path.first().map(|f| f.id.clone()).unwrap_or_default() {
        folder_path.insert(0, root_folder);
    }
    Ok(folder_path)
}


/// Create a new folder (PropNode), link it to the parent folder and set owner.
/// Returns the new folder node.
///
/// Call this inside a transaction (does multiple dependent DB calls)
pub async fn create_folder(srv: &mut GrpcServerConn, ses: &org::UserSessionData, parent_folder: Option<org::PropNode>, args: FoldeBodyData)
    -> RpcResult<org::PropNode>
{
    use org::graph_obj::Id::NodeId;
    let user = ses.user.clone().ok_or(Status::internal("No user in session"))?;

    let tx = TransactionGuard::begin(srv, "create_folder").await?;

    let parent_folder = match parent_folder {
        Some(f) => f,
        None => mkget_user_root_folder(srv, &user).await?
    };

    // Check if folder with same name already exists
    let siblings = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
        graph_rel: Some(org::GraphObjRel {
            rel: Some(ParentIs(org::GraphObj { id: Some(NodeId(parent_folder.id.clone())) })),
            edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
        }),
        ..Default::default()
    }).await?.into_inner();

    for fld in siblings.items.iter() {
        if let Some(b) = &fld.body {
            if let Ok(folder_data) = serde_json::from_str::<FoldeBodyData>(b) {
                if folder_data.name == args.name {
                    return Err(Status::already_exists(format!("Folder '{}' already exists", args.name)));
                }
            } else {
                return Err(Status::internal(format!("Corrupted DB: bad FolderData JSON in folder (id={}) body: {:?}", fld.id, b)));
            }
        } else {
            return Err(Status::internal(format!("Corrupted DB: folder (id={}) has no body", fld.id)));
        }
    }

    // Create folder node
    let folder = srv.db_upsert(
        org::DbUpsertRequest {
            nodes: vec![ org::PropNode {
                body: Some(serde_json::to_string(&args).unwrap()),
                node_type: FOLDER_NODE_TYPE.into(),
                ..Default::default()
            }],
            ..Default::default()
        }).await?.into_inner().nodes.first().cloned().ok_or(Status::internal("Failed to create folder node"))?;

    // Link it to parent
    srv.db_upsert(org::DbUpsertRequest {
        edges: vec![
            org::PropEdge {
                edge_type: PARENT_FOLDER_EDGE_TYPE.into(),
                from: Some(org::GraphObj { id: Some(NodeId(folder.id.clone())) }),
                to: Some(org::GraphObj { id: Some(NodeId(parent_folder.id.clone())) }),
                ..Default::default()
            }
        ],
        ..Default::default()
    }).await?;

    tx.commit().await?;
    Ok(folder)
}


/// Query database for subfolders and videos in given folder.
/// Returned edges are sorted by edge.sort_order
pub async fn fetch_folder_contents(srv: &mut GrpcServerConn, folder: &org::PropNode)
    -> RpcResult<(Vec<org::PropNode>, Vec<proto::Video>, Vec<org::PropEdge>)>
{
    let sub_folders_res = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
        graph_rel: Some(org::GraphObjRel {
            rel: Some(ParentIs(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(folder.id.clone())) })),
            edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
        }),
        ..Default::default() }).await?.into_inner();

    let videos_res = srv.db_get_videos(org::DbGetVideosRequest {
        filter: Some(org::db_get_videos_request::Filter::GraphRel(
            org::GraphObjRel {
                rel: Some(ParentIs(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(folder.id.clone())) })),
                edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
            })),
        ..Default::default() }).await?.into_inner();

    let mut edges = srv.db_get_prop_edges(org::DbGetPropEdgesRequest {
            edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
            to: Some(GraphObj { id: Some(org::graph_obj::Id::NodeId(folder.id.clone())), ..Default::default() }),
            ..Default::default()
        }).await?.into_inner().items;
    edges.sort_by(|a, b| {
            let a = a.sort_order.unwrap_or(f32::NAN);
            let b = b.sort_order.unwrap_or(f32::NAN);
            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Less)
        });

    Ok((sub_folders_res.items, videos_res.items, edges))
}
