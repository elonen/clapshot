use std::collections::HashMap;
use lib_clapshot_grpc::proto::org::{UserSessionData, PropNode};

use lib_clapshot_grpc::proto::{self, org};
use tonic::Status;

use crate::graph_utils::{USER_ID_NODE_TYPE, OWNER_EDGE_TYPE, PATH_COOKIE_NAME, FOLDER_NODE_TYPE, PARENT_FOLDER_EDGE_TYPE, mkget_session_user};
use crate::{GrpcServerConn, RpcResult};



#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct UserNodeData {
    pub(crate) name: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct FolderData {
    pub name: String,
    pub preview_cache: Vec<proto::page_item::folder_listing::Item>
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
            edge_type: Some(OWNER_EDGE_TYPE.into()),
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
/// Returns empty vector if no folder path is set or if the path is invalid.
pub (crate) async fn get_current_folder_path(srv: &mut GrpcServerConn, ses: &UserSessionData)
    -> RpcResult<Vec<org::PropNode>>
{
    let mut folder_path = vec![];
    if let Some(ck) = &ses.cookies {
        if let Some(fp_ids_json) = ck.cookies.get(PATH_COOKIE_NAME) {
            match serde_json::from_str::<Vec<String>>(fp_ids_json) {
                Ok(fp_ids) =>
                {
                    // Get PropNodes for the folder IDs
                    let folders = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
                            node_type: Some("folder".into()),
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
                        // Some folder weren't found in DB. Clear cookie.
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


/// Create a new folder (PropNode), link it to the parent folder and set owner.
/// Returns the new folder node.
/// 
/// Call this inside a transaction (does multiple dependent DB calls)
pub async fn create_folder(srv: &mut GrpcServerConn, ses: &org::UserSessionData, parent_folder: Option<org::PropNode>, args: FolderData)
    -> RpcResult<org::PropNode>
{
    use org::graph_obj::Id::NodeId;
    let user = ses.user.clone().ok_or(Status::internal("No user in session"))?;

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
        None => mkget_session_user(srv, &user).await?
    };
    srv.db_upsert(org::DbUpsertRequest {
        edges: vec![
            org::PropEdge {
                id: "".into(),
                body: None,
                edge_type: OWNER_EDGE_TYPE.into(),
                from: Some(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(new_folder.id.clone())) }),
                to: Some(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(owner_node.id.clone())) }),
                ..Default::default()
            }
        ],
        ..Default::default()
    }).await?;

    Ok(new_folder)
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

