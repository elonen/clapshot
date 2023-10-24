use std::sync::Arc;
use lib_clapshot_grpc::proto::org;
use tokio::sync::Mutex;
use crate::srv_short::{get_parentless_videos, get_all_videos, mk_edge_video_to_node, db_upsert_edges, TransactionGuard, mkget_user_root_folder};
use org::{graph_obj_rel::Rel::ChildIs, graph_obj::Id::VideoId};

use crate::GrpcServerConn;
use crate::graph_utils::{PARENT_FOLDER_EDGE_TYPE, validate_user_id_syntax};


pub type ErrorsPerVideo = Vec<(String, String)>;


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

    // Check for videos that have no parent folder and add one to owner's root folder.
    async fn fix_folderless_videos(srv: Arc<Mutex<GrpcServerConn>>, span: &tracing::Span)
        -> anyhow::Result<ErrorsPerVideo>
    {
        let page_size = 8;
        let mut paging = org::DbPaging { page_size, page_num: 0, };
        let mut errors: ErrorsPerVideo = Vec::new();

        loop {
            let mut srv = srv.lock().await;
            let tx = TransactionGuard::begin(&mut srv, "fix_folderless").await?;
            let mut edges = Vec::new();

            let vids = get_parentless_videos(&mut srv, PARENT_FOLDER_EDGE_TYPE, &mut paging).await?;
            span.in_scope(|| tracing::info!("Found {} folderless videos (page {})", vids.len(), paging.page_num));
            if vids.is_empty() {
                break;
            }

            for v in vids {
                match v.added_by {
                    Some(user) => {
                        validate_user_id_syntax(&user.id)?;
                        let root = mkget_user_root_folder(&mut srv, &user).await?;
                        span.in_scope(|| tracing::info!("Pointing video {} to user's ({}) root folder", v.id, user.id));
                        let edge = mk_edge_video_to_node(PARENT_FOLDER_EDGE_TYPE, &v.id, &root.id);
                        edges.push(edge);
                    },
                    None => {
                        errors.push((v.id.clone(), format!("Video {} has no added_by username", v.id)));
                    }
                }
            }
            db_upsert_edges(&mut srv, edges).await?;
            tx.commit().await?;
            paging.page_num += 1;
        }

        Ok(errors)
    }

    let folderfix_errors: Vec<(String, String)> = fix_folderless_videos(srv.clone(), &span).await?;
    span.in_scope(|| tracing::info!(errors=?folderfix_errors, "DB check complete."));
    Ok(folderfix_errors)
}


/// Test: verify results after a DB check task is complete
pub async fn assert_db_check_postconds(srv: &mut crate::OrganizerOutboundClient<crate::Channel>, _span: tracing::Span) -> anyhow::Result<()>
{
    let all_videos = get_all_videos(srv).await?;
    if all_videos.len() == 0 {
        anyhow::bail!("No videos in test database");
    }

    for v in all_videos {
        // Check that video has at least one parent folder
        let parents = srv.db_get_prop_nodes(org::DbGetPropNodesRequest {
            graph_rel: Some(org::GraphObjRel {
                    rel: Some(ChildIs(org::GraphObj { id: Some(VideoId(v.id.clone())) })),
                    edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
                }), ..Default::default() }).await?.into_inner();

        if parents.items.len() < 1 { anyhow::bail!("Video {} still has no parent folder(s)", v.id); }
        if parents.items.first().unwrap().node_type != crate::graph_utils::FOLDER_NODE_TYPE { anyhow::bail!("Video {} parent is not a folder", v.id); }
    };
    Ok(())
}
