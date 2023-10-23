use std::{default, sync::Arc};
use lib_clapshot_grpc::proto::{org, self};
use tokio::sync::Mutex;
use crate::{GrpcServerConn, graph_utils::{mkget_session_user, validate_user_id_syntax}};

pub type SrvRef = crate::OrganizerOutboundClient<crate::Channel>;

// =================================================================
// Transaction guard
// =================================================================

pub struct TransactionGuard<'a> {
    srv: &'a Arc<Mutex<GrpcServerConn>>,
    active: bool,
    name: String,
}

impl<'a> TransactionGuard<'a> {
    pub async fn begin(srv: &'a Arc<Mutex<GrpcServerConn>>, name: &str) -> anyhow::Result<TransactionGuard<'a>> {
        tracing::debug!("Beginning transaction '{}'", name);
        srv.lock().await.db_begin_transaction(org::DbBeginTransactionRequest {}).await?;
        Ok(TransactionGuard { srv, active: true, name: name.into() })
    }

    pub async fn commit(mut self) -> anyhow::Result<()> {
        if !self.active { return Err(anyhow::anyhow!("TransactionGuard: commit() called twice")); }
        tracing::debug!("Committing transaction '{}'", self.name);
        self.srv.lock().await.db_commit_transaction(org::DbCommitTransactionRequest {}).await?;
        self.active = false; // Ensure it doesn't rollback when dropped
        Ok(())
    }
}

impl<'a> Drop for TransactionGuard<'a> {
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

pub async fn db_upsert_edges(srv: &Arc<Mutex<GrpcServerConn>>, edges: Vec<org::PropEdge>) -> anyhow::Result<()> {
    tracing::debug!("Upserting {} edges", edges.len());
    srv.lock().await.db_upsert(org::DbUpsertRequest { edges, ..default::Default::default() } ).await?;
    Ok(())
}

pub async fn get_childless_videos(srv: &Arc<Mutex<GrpcServerConn>>, edge_type: &str, paging: &mut org::DbPaging)
    -> anyhow::Result<Vec<proto::Video>>
{
    let vids = srv.lock().await.db_get_videos(org::DbGetVideosRequest {
        filter: Some(org::db_get_videos_request::Filter::GraphRel(
            org::GraphObjRel {
                rel: Some(org::graph_obj_rel::Rel::Childless(proto::Empty {})),
                edge_type: Some(edge_type.into()),
            })),
        paging: Some(paging.clone()),
    }).await?.into_inner().items;
    Ok(vids)
}

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
                Ok(unode) => Ok(unode),
                Err(e) => {
                    anyhow::bail!("Failed to add user node '{}': {}", user.id, e);
                }
            }
        }
    }
}
