use std::sync::Arc;
use lib_clapshot_grpc::proto::org;
use tokio::sync::Mutex;
use crate::srv_short::{getcheck_video_owner, get_childless_videos, get_all_videos, mk_edge_video_to_node, commit_transaction, begin_transaction, db_upsert_edges, mkget_video_owner_node};

use crate::GrpcServerConn;
use crate::graph_utils::OWNER_EDGE_TYPE;


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



async fn fix_dangling_videos(srv: Arc<Mutex<GrpcServerConn>>, span: tracing::Span)
    -> anyhow::Result<ErrorsPerVideo>
{
    span.in_scope(|| tracing::info!("DB check in progress..."));

    /// Check for videos that have no owner edge and add one.
    async fn check_owner_edges(srv: Arc<Mutex<GrpcServerConn>>, span: &tracing::Span)
        -> anyhow::Result<ErrorsPerVideo>
    {
        begin_transaction(&srv).await?;

        let page_size = 8;
        let mut paging = org::DbPaging { page_size, page_num: 0, };
        let mut errors: ErrorsPerVideo = Vec::new();

        loop {
            begin_transaction(&srv).await?;
            let mut edges = Vec::new();

            let vids = get_childless_videos(&srv, OWNER_EDGE_TYPE, &mut paging).await?;
            span.in_scope(|| tracing::info!("Found {} ownerless videos (page {})", vids.len(), paging.page_num));
            if vids.is_empty() {
                break;
            }

            for v in vids {
                span.in_scope(|| tracing::info!("Adding owner edge to video {}", v.id));
                match mkget_video_owner_node(&srv, &v).await {
                    Ok(owner_node) => {
                        let edge = mk_edge_video_to_node(OWNER_EDGE_TYPE, &v.id, &owner_node.id);
                        edges.push(edge);
                    },
                    Err(e) => {
                        errors.push((v.id.clone(), format!("Failed to add owner edge: {}", e)));
                    }
                }
            }

            db_upsert_edges(&srv, edges).await?;
            commit_transaction(&srv).await?;
            paging.page_num += 1;
        }

        commit_transaction(&srv).await?;
        Ok(errors)
    }

    let owner_edge_errors: Vec<(String, String)> = check_owner_edges(srv, &span).await?;
    span.in_scope(|| tracing::info!(errors=?owner_edge_errors, "DB check complete."));
    Ok(owner_edge_errors)
}



/// Test: verify results after a DB check task is complete
pub async fn assert_db_check_postconds(srv: &mut crate::OrganizerOutboundClient<crate::Channel>, _span: tracing::Span) -> anyhow::Result<()>
{
    let all_videos = get_all_videos(srv).await?;
    if all_videos.len() == 0 {
        anyhow::bail!("No videos in test database");
    }
    for v in all_videos {
        getcheck_video_owner(srv, &v.id).await?;
    };
    Ok(())
}
