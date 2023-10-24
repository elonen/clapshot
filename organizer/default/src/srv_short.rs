use std::{default, sync::Arc};
use lib_clapshot_grpc::proto::{org, self};
use tokio::sync::Mutex;
use tonic::Status;
use crate::{GrpcServerConn, RpcResult, folder_ops::FolderData};

pub type SrvRef = crate::OrganizerOutboundClient<crate::Channel>;

// =================================================================
// Transaction guard
// =================================================================

pub struct TransactionGuard {
    srv: Arc<Mutex<GrpcServerConn>>,
    active: bool,
    name: String,
}

impl<'a> TransactionGuard {
    pub async fn begin(srv: &mut GrpcServerConn, name: &str) -> RpcResult<TransactionGuard> {
        TransactionGuard::begin_from_arc(Arc::new(Mutex::new(srv.clone())), name).await
    }

    pub async fn begin_from_arc(srv: Arc<Mutex<GrpcServerConn>>, name: &str) -> RpcResult<TransactionGuard> {
        tracing::debug!("Beginning transaction '{}'", name);
        srv.lock().await.db_begin_transaction(org::DbBeginTransactionRequest {}).await?;
        Ok(TransactionGuard { srv, active: true, name: name.into() })
    }

    pub async fn commit(mut self) -> RpcResult<()> {
        if !self.active { return Err(Status::internal("Transaction already committed")); }
        tracing::debug!("Committing transaction '{}'", self.name);
        self.srv.lock().await.db_commit_transaction(org::DbCommitTransactionRequest {}).await?;
        self.active = false; // Ensure it doesn't rollback when dropped
        Ok(())
    }
}

impl<'a> Drop for TransactionGuard {
    fn drop(&mut self) {
        if self.active {
            tracing::debug!("Rolling back transaction '{}'", self.name);
            let srv_clone = self.srv.clone();
            // Check if Tokio runtime is still active
            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::spawn(async move {
                    let _ = srv_clone.lock().await.db_rollback_transaction(org::DbRollbackTransactionRequest {}).await;
                });
            }
        }
    }
}

// =================================================================
// Trivial helpers to shorten syntax
// =================================================================

pub async fn db_upsert_edges(srv: &mut GrpcServerConn, edges: Vec<org::PropEdge>) -> anyhow::Result<()> {
    tracing::debug!("Upserting {} edges", edges.len());
    srv.db_upsert(org::DbUpsertRequest { edges, ..default::Default::default() } ).await?;
    Ok(())
}

pub async fn get_parentless_videos(srv: &mut GrpcServerConn, edge_type: &str, paging: &mut org::DbPaging)
    -> anyhow::Result<Vec<proto::Video>>
{
    let vids = srv.db_get_videos(org::DbGetVideosRequest {
        filter: Some(org::db_get_videos_request::Filter::GraphRel(
            org::GraphObjRel {
                rel: Some(org::graph_obj_rel::Rel::Parentless(proto::Empty {})),
                edge_type: Some(edge_type.into()),
            })),
        paging: Some(paging.clone()),
    }).await?.into_inner().items;
    Ok(vids)
}

/*
pub async fn get_childless_videos(srv: &mut GrpcServerConn, edge_type: &str, paging: &mut org::DbPaging)
    -> anyhow::Result<Vec<proto::Video>>
{
    let vids = srv.db_get_videos(org::DbGetVideosRequest {
        filter: Some(org::db_get_videos_request::Filter::GraphRel(
            org::GraphObjRel {
                rel: Some(org::graph_obj_rel::Rel::Childless(proto::Empty {})),
                edge_type: Some(edge_type.into()),
            })),
        paging: Some(paging.clone()),
    }).await?.into_inner().items;
    Ok(vids)
}
*/

pub fn mk_edge_video_to_node(edge_type: &str, video_id: &str, node_id: &str) -> org::PropEdge
{
    org::PropEdge {
        from: Some(org::GraphObj { id: Some(org::graph_obj::Id::VideoId(video_id.into())) }),
        to: Some(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(node_id.into())) }),
        edge_type: edge_type.into(),
        ..default::Default::default()
    }
}


pub async fn get_all_videos(srv: &mut SrvRef)
    -> anyhow::Result<Vec<proto::Video>>
{
    let vids = srv.db_get_videos(org::DbGetVideosRequest {
        filter: Some(org::db_get_videos_request::Filter::All(proto::Empty {})),
         ..default::Default::default()
        }).await?.into_inner();
    Ok(vids.items)
}

/*
pub async fn getcheck_video_owner(srv: &mut SrvRef, video_id: &str)
    -> anyhow::Result<org::PropNode>
{
    let owner_proplist = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
        graph_rel: Some(org::GraphObjRel {
            rel: Some(org::graph_obj_rel::Rel::ChildOf(
                org::GraphObj { id: Some(org::graph_obj::Id::VideoId(video_id.into())) })),
            edge_type: Some(crate::graph_utils::OWNER_EDGE_TYPE.into()),
        }), ..Default::default() }).await?.into_inner();

    if owner_proplist.items.len() != 1 {
        anyhow::bail!("Video {} has {} owners, not 1", video_id, owner_proplist.items.len());
    }
    Ok(owner_proplist.items[0].clone())
}
*/


/*
pub async fn mkget_video_owner_node(srv: &Arc<Mutex<GrpcServerConn>>, video: &proto::Video)
-> anyhow::Result<org::PropNode>
{
    match &video.added_by {
        None => anyhow::bail!("No user in video"),
        Some(user) => {
            if let Err(e) = validate_user_id_syntax(&user.id) {
                anyhow::bail!("Invalid user ID '{}': {}", user.id, e);
            }
            match mkget_session_user(&mut *srv.lock().await, &user).await {
                Ok(unode) => {
                    tracing::debug!("User node for video {}: {:?}", video.id, unode);
                    Ok(unode)
                },
                Err(e) => {
                    anyhow::bail!("Failed to add user node '{}': {}", user.id, e);
                }
            }
        }
    }
}
*/

pub async fn mkget_user_root_folder(srv: &mut GrpcServerConn, user: &proto::UserInfo)
    -> RpcResult<org::PropNode>
{
    //let user_node = mkget_session_user(srv, &user).await?;
    let root_key = format!("|root|{}", user.id);

    match srv.db_get_singleton_prop_node(org::DbGetSingletonPropNodeRequest {
            node_type: crate::graph_utils::FOLDER_NODE_TYPE.into(),
            singleton_key: root_key.clone()
        }).await?.into_inner().node
    {
        Some(root_node) => {
            Ok(root_node)
        },
        None => {
            let root_folder = srv.db_upsert(org::DbUpsertRequest {
                nodes: vec![
                    org::PropNode {
                        id: "".into(),  // empty = insert
                        body: Some(serde_json::to_string(&FolderData { name: "root".into(), ..Default::default() }).unwrap()),
                        node_type: crate::graph_utils::FOLDER_NODE_TYPE.into(),
                        singleton_key: Some(root_key),
                    }
                ],
                ..Default::default()
            }).await?.into_inner();
            Ok(root_folder.nodes[0].clone())
        }
    }
}
