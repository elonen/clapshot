use std::collections::{HashMap, VecDeque};
use std::default;
use std::sync::Arc;
use lib_clapshot_grpc::proto::org::{DbUpsertRequest};
use lib_clapshot_grpc::proto::{org, self};
use tokio::sync::Mutex;
use tracing::Instrument;


use crate::GrpcServerConn;
use crate::graph_utils::{OWNER_EDGE_TYPE, validate_user_id_syntax, mkget_session_user};


pub type VideoId = String;
pub type ErrorsPerVideo = Vec<(VideoId, String)>;


/// Check the database for missing / malformed metadata and fix it.
///
/// This is a long running background task. The returned Option will be
/// set to Some when the task is complete.
///
/// The HashMap inside 'res' will contain error messages for videos that could not be fixed.
pub fn spawn_database_check(
    srv: Arc<Mutex<GrpcServerConn>>,
    res: Arc<Mutex<Option<anyhow::Result<ErrorsPerVideo>>>>)
{
    tokio::spawn(async move {
        let span = tracing::info_span!("db_check");
        let r = fix_dangling_videos(srv, span).await;
        res.lock().await.replace(r);
    });
}


/// Iterate over videos that have no incoming edges of the given type.
/// Handles paging automatically and opens/commits transactions as needed.
struct ChildlessVideoIter {
    srv: Arc<Mutex<GrpcServerConn>>,
    edge_type: String,
    paging: org::DbPaging,
    buffer: VecDeque<proto::Video>,
    transaction: bool,
}
impl ChildlessVideoIter {
    async fn new(srv: Arc<Mutex<GrpcServerConn>>, edge_type: String, page_size: u32) -> Self {
        ChildlessVideoIter { srv, edge_type,
            paging: org::DbPaging { page_size, page_num: 0, },
            buffer: VecDeque::new(),
            transaction: false,
        }
    }
    /// Commit the current transaction, if any.
    /// Call this before dropping the iterator to avoid leaving transactions open;
    /// otherwise they will be rolled back.
    async fn commit(self: &mut Self) -> anyhow::Result<()> {
        if self.transaction {
            tracing::debug!("Committing transaction");
            self.srv.lock().await.db_commit_transaction(org::DbCommitTransactionRequest {}).await?;
            self.transaction = false;
        }
        Ok(())
    }

    /// Return the next video, or None if there are no more.
    async fn next(&mut self) -> anyhow::Result<Option<proto::Video>> {
        if self.buffer.is_empty() {

            self.commit().await?;
            tracing::debug!("Starting new transaction");
            self.srv.lock().await.db_begin_transaction(org::DbBeginTransactionRequest {}).await?;
            self.transaction = true;

            self.buffer = self.srv.lock().await.db_get_videos(org::DbGetVideosRequest {
                filter: Some(org::db_get_videos_request::Filter::GraphRel(
                    org::GraphObjRel {
                        rel: Some(org::graph_obj_rel::Rel::Childless(proto::Empty {})),
                        edge_type: Some(self.edge_type.clone()),
                    })),
                paging: Some(self.paging.clone()),
            }).await?.into_inner().items.into();
            self.paging.page_num += 1;
        }
        Ok(self.buffer.pop_front())
    }
}

impl Drop for ChildlessVideoIter {
    fn drop(&mut self) {
        if self.transaction {
            tracing::warn!("Dropping ChildlessVideoIter with active transaction, rolling back");
            let srv = self.srv.clone();
            tokio::spawn(async move {
                if let Err(e) = srv.lock().await.db_rollback_transaction(org::DbRollbackTransactionRequest {}).await {
                    tracing::error!("Failed to rollback transaction: {}", e);
                }
            });
        }
    }
}


async fn fix_dangling_videos(srv: Arc<Mutex<GrpcServerConn>>, span: tracing::Span)
    -> anyhow::Result<ErrorsPerVideo>
{
    span.in_scope(|| tracing::info!("DB check in progress..."));

    const BATCH_SIZE: u32 = 8;

    /// Helper for adding edges to the database in batches,
    /// collecting error messages and caching user nodes.
    struct EdgeBatchManager {
        srv: Arc<Mutex<GrpcServerConn>>,
        user_id_nodes: HashMap<String, org::PropNode>,
        video_errors: ErrorsPerVideo,
        edge_buffer: Vec<org::PropEdge>,
        span: tracing::Span,
    }

    impl EdgeBatchManager {
        async fn new(srv: Arc<Mutex<GrpcServerConn>>, span: tracing::Span) -> Self {
            Self { srv, user_id_nodes: HashMap::new(), video_errors: ErrorsPerVideo::new(), edge_buffer: Vec::new(), span }
        }

        /// Make sure the user node exists and return its ID, caching it for future use
        async fn mkget_owner_node(self: &mut Self, video: &proto::Video) -> Option<org::PropNode> {
            match &video.added_by {
                None => {
                    self.video_errors.push((video.id.clone(), "No user in video".into()));
                    None
                },
                Some(user) => {
                    if let Err(e) = validate_user_id_syntax(&user.id) {
                        self.video_errors.push((video.id.clone(), format!("Invalid user ID '{}': {}", user.id, e)));
                        return None;
                    }
                    match self.user_id_nodes.get(&user.id) {
                        Some(unode) => Some(unode.clone()),
                        None => {
                            match mkget_session_user(&mut *self.srv.lock().await, &user).await {
                                Ok(unode) => {
                                    self.user_id_nodes.insert(user.id.clone(), unode.clone());
                                    Some(unode)
                                },
                                Err(e) => {
                                    self.video_errors.push((video.id.clone(), format!("Failed to add user node '{}': {}", user.id, e)));
                                    None
                                }
                        }}}}}
        }

        /// Add an edge to the buffer, flushing the buffer if it's full
        async fn add_edge(self: &mut Self, edge: org::PropEdge) {
            self.span.in_scope(|| tracing::debug!("Adding edge: {:?}", edge));
            self.edge_buffer.push(edge);
            if self.edge_buffer.len() >= BATCH_SIZE as usize {
                self.flush_inserts().await;
            }
        }

        /// Execute buffered inserts, clearing the buffer.
        async fn flush_inserts(self: &mut Self) {
            if !self.edge_buffer.is_empty() {
                if let Err(e) = self.srv.lock().await.db_upsert(DbUpsertRequest {
                    edges: self.edge_buffer.clone(),
                    ..default::Default::default()
                }).await {
                    for g in self.edge_buffer.iter().filter_map(|e| e.from.clone()) {
                        if let Some(org::graph_obj::Id::VideoId(v)) = g.id {
                            self.video_errors.push((v.clone(), format!("Failed to edge from video: {}", e)));
                        }}}
                self.edge_buffer.clear();
                self.span.in_scope(|| tracing::debug!("Edge buffer flushed"));
            } else {
                self.span.in_scope(|| tracing::debug!("Edge buffer empty, nothing to flush"));
            }
        }
    }

    async fn check_owner_edges(srv: Arc<Mutex<GrpcServerConn>>, span: &tracing::Span)
        -> anyhow::Result<ErrorsPerVideo>
    {
        let mut vid_iter = ChildlessVideoIter::new(srv.clone(), OWNER_EDGE_TYPE.into(), BATCH_SIZE).await;
        let mut owner_ebm = EdgeBatchManager::new(srv.clone(), span.clone()).await;
        while let Some(v) = vid_iter.next().instrument(span.clone()).await? {
            span.in_scope(|| tracing::debug!("Checking video {}", v.id));
            if let Some(user_node) = owner_ebm.mkget_owner_node(&v).await {
                owner_ebm.add_edge(org::PropEdge {
                    from: Some(org::GraphObj { id: Some(org::graph_obj::Id::VideoId(v.id.clone())) }),
                    to: Some(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(user_node.id.clone())) }),
                    edge_type: OWNER_EDGE_TYPE.into(),
                    ..default::Default::default()
                }).await;
            }
        }
        owner_ebm.flush_inserts().instrument(span.clone()).await;
        vid_iter.commit().instrument(span.clone()).await?;
        Ok(owner_ebm.video_errors)
    }

    let owner_edge_errors = check_owner_edges(srv, &span).await?;
    span.in_scope(|| tracing::info!(errors=?owner_edge_errors, "DB check complete."));
    Ok(owner_edge_errors)
}



/// Test: verify results after a DB check task is complete
pub async fn assert_db_check_postconds(srv: &mut crate::OrganizerOutboundClient<crate::Channel>, _span: tracing::Span) -> anyhow::Result<()>
{
    let all_videos = srv.db_get_videos(org::DbGetVideosRequest {
        filter: Some(org::db_get_videos_request::Filter::All(proto::Empty {})),
         ..default::Default::default()
        }).await?.into_inner();

    if all_videos.items.len() == 0 {
        anyhow::bail!("No videos in test database");
    }

    for v in all_videos.items {
        let owner_proplist = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
            graph_rel: Some(org::GraphObjRel {
                rel: Some(org::graph_obj_rel::Rel::ParentOf(
                    org::GraphObj { id: Some(org::graph_obj::Id::VideoId(v.id.clone())) })),
                edge_type: Some(crate::graph_utils::OWNER_EDGE_TYPE.into()),
            }), ..Default::default() }).await?.into_inner();

        if owner_proplist.items.len() != 1 {
            anyhow::bail!("Video {} has {} owners, not 1", v.id, owner_proplist.items.len());
        }
    };

    Ok(())
}