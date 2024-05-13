#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::collections::HashMap;
use std::sync::Arc;
use std::str::FromStr;
use lib_clapshot_grpc::proto::client::ClientToServerCmd;
use lib_clapshot_grpc::proto::client::client_to_server_cmd::{ListMyVideos, OpenVideo, DelVideo, RenameVideo, EditComment, DelComment, JoinCollab, LeaveCollab, CollabReport, ReorderItems};
use parking_lot::RwLock;
type WsMsg = warp::ws::Message;

type Res<T> = anyhow::Result<T>;
type MsgSender = tokio::sync::mpsc::UnboundedSender<WsMsg>;
type SenderList = Vec<MsgSender>;
type SenderListMap = Arc<RwLock<HashMap<String, SenderList>>>;

use serde_json::json;
use anyhow::{anyhow, bail, Context};

use data_url::{DataUrl, mime};
use sha2::{Sha256, Digest};
use hex;

use super::user_session::{self, AuthzTopic, org_authz_with_default};

use super::UserSession;

use crate::api_server::server_state::ServerState;
use crate::api_server::user_session::Topic;
use crate::database::error::DBError;
use crate::database::{models, DB, DbBasicQuery, DbQueryByUser, DbQueryByVideo, DBPaging};
use crate::database::schema::comments::drawing;
use crate::{send_user_error, send_user_ok, client_cmd};

use lib_clapshot_grpc::proto;

use proto::org::authz_user_action_request as authz_req;


/// Get video by ID from DB, or send user error.
/// Return None if video not found and error was sent, or Some(video) if found.
async fn get_video_or_send_error(video_id: Option<&str>, ses: &Option<&mut UserSession>, server: &ServerState) -> Res<Option<models::Video>> {
    let video_id = video_id.ok_or(anyhow!("video id missing"))?;

    match models::Video::get(&mut server.db.conn()?, &video_id.into()) {
        Err(DBError::NotFound()) => {
            if let Some(ses) = ses {
                send_user_error!(ses.user_id, server, Topic::Video(video_id), "No such video.");
            };
            Ok(None)
        }
        Err(e) => { bail!(e); }
        Ok(v) => { Ok(Some(v)) }
    }
}

// ---------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------

/// Send user a list of all videos they have.
pub async fn msg_list_my_videos(data: &ListMyVideos , ses: &mut UserSession, server: &ServerState) -> Res<()> {
    org_authz_with_default(&ses.org_session, "list videos", true, server,
        &ses.organizer, true, AuthzTopic::Other(None, authz_req::other_op::Op::ViewHome)).await?;

    // Try to delegate request to Organizer.
    if let Some(org) = &ses.organizer {
        let req = proto::org::NavigatePageRequest {
            ses: Some(ses.org_session.clone()),
        };
        match org.lock().await.navigate_page(req).await {
            Err(e) => {
                if e.code() == tonic::Code::Unimplemented {
                    tracing::debug!("Organizer doesn't implement navigate_page(). Using default.");
                } else {
                    tracing::error!(err=?e, "Error in organizer navigate_page() call");
                    anyhow::bail!("Error in navigate_page() organizer call: {:?}", e);
                }
            },
            Ok(res) => {
                server.emit_cmd(
                    client_cmd!(ShowPage, {
                        page_items: res.into_inner().page_items,
                    }),
                    super::SendTo::UserSession(&ses.sid))?;
                return Ok(());
            }
        }
    }

    // Organizer didn't handle this, so return a default listing.
    let videos = models::Video::get_by_user(&mut server.db.conn()?, &ses.user_id, DBPaging::default())?;
    let h_txt = if videos.is_empty() {
        "<h2>You have no videos yet.</h2>"
    } else {
        "<h2>All your videos</h2>"
    };
    let heading = proto::PageItem{ item: Some(proto::page_item::Item::Html(h_txt.into()))};
    let listing = crate::grpc::folder_listing_for_videos(&videos, &server.url_base);
    let page = vec![heading, listing];

    server.emit_cmd(
        client_cmd!(ShowPage, { page_items: page }),
        super::SendTo::UserSession(&ses.sid))?;
    Ok(())
}


/// User opens a video.
/// Send them the video info and all comments related to it.
/// Register the session as a viewer of the video (video_session_guard).
pub async fn msg_open_video(data: &OpenVideo, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(v) = get_video_or_send_error(Some(&data.video_id), &Some(ses), server).await? {
        org_authz_with_default(&ses.org_session,
            "open video", true, server, &ses.organizer,
            true, AuthzTopic::Video(&v, authz_req::video_op::Op::View)).await?;
        send_open_video_cmd(server, &ses.sid, &v.id).await?;
        ses.cur_video_id = Some(v.id);
    }
    Ok(())
}


pub async fn send_open_video_cmd(server: &ServerState, session_id: &str, video_id: &str) -> Res<()> {
    server.link_session_to_video(session_id, video_id)?;
    let v = models::Video::get(&mut server.db.conn()?, &video_id.into())?.to_proto3(&server.url_base);
    if v.playback_url.is_none() {
        return Err(anyhow!("No video file"));
    }
    server.emit_cmd(
        client_cmd!(OpenVideo, {video: Some(v)}),
        super::SendTo::UserSession(session_id))?;
    let mut cmts = vec![];
    for mut c in models::Comment::get_by_video(&mut server.db.conn()?, video_id, DBPaging::default())? {
        server.fetch_drawing_data_into_comment(&mut c).await?;
        cmts.push(c.to_proto3());
    }
    server.emit_cmd(
        client_cmd!(AddComments, {comments: cmts}),
        super::SendTo::UserSession(session_id))?;
    Ok(())
}


pub async fn del_video_and_cleanup(video_id: &str, ses: Option<&mut UserSession>, server: &ServerState) -> Res<()> {
    if let Some(v) = get_video_or_send_error(Some(video_id), &ses, server).await? {

        // Check authorization against user session, if provided
        if let Some(ses) = &ses {
            let default_perm = ses.user_id == (&v).user_id || ses.is_admin;
            org_authz_with_default(&ses.org_session, "delete video", true, server, &ses.organizer,
                default_perm, AuthzTopic::Video(&v, authz_req::video_op::Op::Delete)).await?;
        }

        models::Video::delete(&mut server.db.conn()?, &v.id)?;
        let mut details = format!("Added by '{}' on {}. Filename was {}.",
            v.user_id.clone(),
            v.added_time,
            v.orig_filename.clone().unwrap_or_default());

        fn backup_video_db_row(server: &ServerState, v: &models::Video) -> Res<()> {
            let backup_file = server.videos_dir.join(v.id.clone()).join("db_backup.json");
            if backup_file.exists() {
                std::fs::remove_file(&backup_file)?;
            }
            let json_str = serde_json::to_string_pretty(&v)?;
            std::fs::write(&backup_file, json_str)?;
            Ok(())
        }

        fn move_video_to_trash(server: &ServerState, video_id: &str) -> Res<()>
        {
            let video_dir = server.videos_dir.join(video_id);
            let trash_dir = server.videos_dir.join("trash");
            if !trash_dir.exists() {
                std::fs::create_dir(&trash_dir)?;
            }
            let hash_and_datetime = format!("{}_{}", video_id, chrono::Utc::now().format("%Y%m%d-%H%M%S"));
            let video_trash_dir = trash_dir.join(hash_and_datetime);
            std::fs::rename(&video_dir, &video_trash_dir)?;
            Ok(())
        }

        let mut cleanup_errors = false;
        if let Err(e) = backup_video_db_row(server, &v) {
            details.push_str(&format!(" WARNING: DB row backup failed: {:?}.", e));
            cleanup_errors = true;

        }
        if let Err(e) = move_video_to_trash(server, &v.id) {
            details.push_str(&format!(" WARNING: Move to trash failed: {:?}.", e));
            cleanup_errors = true;
        }

        if let Some(ses) = ses {
            send_user_ok!(&ses.user_id, &server, Topic::Video(&v.id),
                if !cleanup_errors {"Video deleted."} else {"Video deleted, but cleanup had errors."},
                details, true);
        }
    }
    Ok(())
}


pub async fn msg_del_video(data: &DelVideo, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    del_video_and_cleanup(&data.video_id, Some(ses), server).await
}


pub async fn msg_rename_video(data: &RenameVideo, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(v) = get_video_or_send_error(Some(&data.video_id), &Some(ses), server).await? {
        let default_perm = ses.user_id == (&v).user_id || ses.is_admin;
        org_authz_with_default(&ses.org_session, "rename video", true, server, &ses.organizer,
            default_perm, AuthzTopic::Video(&v, authz_req::video_op::Op::Rename)).await?;

        let new_name = data.new_name.trim();
        if new_name.is_empty() || !new_name.chars().any(|c| c.is_alphanumeric()) {
            send_user_error!(&ses.user_id, server, Topic::Video(&v.id), "Invalid video name (must have letters/numbers)");
            return Ok(());
        }
        if new_name.len() > 160 {
            send_user_error!(&ses.user_id, server, Topic::Video(&v.id), "Video name too long (max 160)");
            return Ok(());
        }
        models::Video::rename(&mut server.db.conn()?, &v.id, new_name)?;
        send_user_ok!(&ses.user_id, server, Topic::Video(&v.id), "Video renamed.",
            format!("New name: '{}'", new_name), true);
    }
    Ok(())
}


pub async fn msg_add_comment(data: &proto::client::client_to_server_cmd::AddComment, ses: &mut UserSession, server: &ServerState) -> Res<()> {

    let video_id = match get_video_or_send_error(Some(&data.video_id), &Some(ses), server).await? {
        Some(v) => {
            let default_perm = ses.user_id == (&v).user_id || ses.is_admin;
            org_authz_with_default(&ses.org_session, "comment video", true, server, &ses.organizer,
                default_perm, AuthzTopic::Video(&v, authz_req::video_op::Op::Comment)).await?;
            v.id
        },
        None => return Ok(()),
    };

    // Parse drawing data if present and write to file
    let mut drwn = data.drawing.clone();
    if let Some(d) = &drwn {
        if d.starts_with("data:") {

            // Convert data URI to bytes
            let img_uri = DataUrl::process(&d).map_err(|e| anyhow!("Invalid drawing data URI"))?;

            if img_uri.mime_type().type_ != "image" || img_uri.mime_type().subtype != "webp" {
                bail!("Invalid mimetype in drawing: {:?}", img_uri.mime_type())
            }
            let img_data = img_uri.decode_to_vec().map_err(|e| anyhow!("Failed to decode drawing data URI: {:?}", e))?;

            // Make up a filename
            fn sha256hex( data: &[u8] ) -> String {
                let mut hasher = Sha256::new();
                hasher.update(data);
                let result = hasher.finalize();
                hex::encode(result)
            }
            let short_csum = sha256hex(img_data.0.as_ref())[..16].to_string();
            let fname = format!("{}.webp", short_csum);

            // Write to file
            let drawing_path = server.videos_dir.join(&video_id).join("drawings").join(&fname);
            std::fs::create_dir_all(drawing_path.parent().unwrap())
                .map_err(|e| anyhow!("Failed to create drawings dir: {:?}", e))?;
            async_std::fs::write(drawing_path, img_data.0).await.map_err(
                |e| anyhow!("Failed to write drawing file: {:?}", e))?;

            // Replace data URI with filename
            drwn = Some(fname);
        }
    };

    let parent_id = match data.parent_id.as_ref().map(|s| s.parse::<i32>()) {
        Some(Ok(id)) => Some(id),
        Some(Err(_)) => { bail!("Invalid parent_id for comment"); }
        None => None,
    };

    let c = models::CommentInsert {
        video_id: video_id.to_string(),
        parent_id,
        user_id: Some(ses.user_id.clone()),
        username_ifnull: ses.user_name.clone(),
        comment: data.comment.clone(),
        timecode: data.timecode.clone(),
        drawing: drwn.clone(),
    };
    let c = models::Comment::insert(&mut server.db.conn()?, &c)
        .map_err(|e| anyhow!("Failed to add comment: {:?}", e))?;
    // Send to all clients watching this video
    ses.emit_new_comment(server, c, super::SendTo::VideoId(&video_id)).await?;
    Ok(())
}


pub async fn msg_edit_comment(data: &EditComment, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let id = i32::from_str(&data.comment_id)?;
    let conn = &mut server.db.conn()?;

    match models::Comment::get(conn, &id) {
        Ok(old) => {
            let default_perm = Some(&ses.user_id) == old.user_id.as_ref() || ses.is_admin;
            org_authz_with_default(&ses.org_session, "edit comment", true, server, &ses.organizer,
                default_perm, AuthzTopic::Comment(&old, authz_req::comment_op::Op::Edit)).await?;

            let vid = &old.video_id;
            models::Comment::edit(conn, id, &data.new_comment)?;

            server.emit_cmd(
                client_cmd!(DelComment, {comment_id: id.to_string()}),
                super::SendTo::VideoId(&vid))?;

            let c = models::Comment::get(conn, &id)?;
            ses.emit_new_comment(server, c, super::SendTo::VideoId(&vid)).await?;
        }
        Err(DBError::NotFound()) => {
            send_user_error!(&ses.user_id, server, Topic::None, "Failed to edit comment.", "No such comment. Cannot edit.", true);
        }
        Err(e) => { bail!(e); }
    }
    Ok(())
}


pub async fn msg_del_comment(data: &DelComment, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let id = i32::from_str(&data.comment_id)?;
    match models::Comment::get(&mut server.db.conn()?, &id) {
        Ok(cmt) => {
            let default_perm = Some(&ses.user_id) == cmt.user_id.as_ref() || ses.is_admin;
            org_authz_with_default(&ses.org_session, "delete comment", true, server, &ses.organizer,
                default_perm, AuthzTopic::Comment(&cmt, authz_req::comment_op::Op::Delete)).await?;

            let vid = cmt.video_id;
            if Some(&ses.user_id) != cmt.user_id.as_ref() && !ses.is_admin {
                send_user_error!(&ses.user_id, server, Topic::Video(&vid), "Failed to delete comment.", "You can only delete your own comments", true);
                return Ok(());
            }
            let all_comm = models::Comment::get_by_video(&mut server.db.conn()?, &vid, DBPaging::default())?;
            if all_comm.iter().any(|c| c.parent_id.map(|i| i.to_string()) == Some(id.to_string())) {
                send_user_error!(&ses.user_id, server, Topic::Video(&vid), "Failed to delete comment.", "Comment has replies. Cannot delete.", true);
                return Ok(());
            }
            models::Comment::delete(&mut server.db.conn()?, &id)?;
            server.emit_cmd(
                client_cmd!(DelComment, {comment_id: id.to_string()}),
                super::SendTo::VideoId(&vid))?;
        }
        Err(DBError::NotFound()) => {
            send_user_error!(&ses.user_id, server, Topic::None, "Failed to delete comment.", "No such comment. Cannot delete.", true);
        }
        Err(e) => { bail!(e); }
    }
    Ok(())
}


pub async fn msg_list_my_messages(data: &proto::client::client_to_server_cmd::ListMyMessages, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let conn = &mut server.db.conn()?;
    let msgs = models::Message::get_by_user(conn, &ses.user_id, DBPaging::default())?;
    server.emit_cmd(
        client_cmd!(ShowMessages, { msgs: (&msgs).into_iter().map(|m| m.to_proto3()).collect() }),
        super::SendTo::UserSession(&ses.sid)
    )?;
    for m in msgs {
        if !m.seen { models::Message::set_seen(conn, m.id, true)?; }
    }
    Ok(())
}


pub async fn msg_join_collab(data: &JoinCollab, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(collab_id) = ses.cur_collab_id.clone() {
        if server.sender_is_collab_participant(collab_id.as_str(), &ses.sender) {
            tracing::debug!("{} is already in collab {}. Ignoring double join.", ses.user_name, collab_id);
            return Ok(());
        }
    }
    ses.collab_session_guard = None;
    ses.cur_collab_id = None;

    if let Some(v) = get_video_or_send_error(Some(&data.video_id), &Some(ses), server).await? {
        org_authz_with_default(&ses.org_session, "join collab", true, server, &ses.organizer,
            true, AuthzTopic::Other(Some(&data.collab_id), authz_req::other_op::Op::JoinCollabSession)).await?;

        match server.link_session_to_collab(&data.collab_id, &v.id, ses.sender.clone()) {
            Ok(csg) => {
                ses.collab_session_guard = Some(csg);
                ses.cur_collab_id = Some(data.collab_id.clone());
                server.emit_cmd(
                    client_cmd!(ShowMessages, { msgs: vec![
                            proto::UserMessage {
                            r#type: proto::user_message::Type::Ok as i32,
                            message: format!("'{}' joined collab", &ses.user_name),
                            ..Default::default()
                        }]
                    }),
                    super::SendTo::Collab(&data.collab_id)
                )?;
            }
            Err(e) => {
                send_user_error!(&ses.user_id, server, Topic::Video(&v.id), format!("Failed to join collab session: {}", e));
            }
        }
    }
    Ok(())
}


pub async fn msg_leave_collab(data: &LeaveCollab, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(collab_id) = &ses.cur_collab_id {
        server.emit_cmd(
            client_cmd!(ShowMessages, { msgs: vec![
                    proto::UserMessage {
                    r#type: proto::user_message::Type::Ok as i32,
                    message: format!("'{}' left collab", &ses.user_name),
                    ..Default::default()
                }]
            }),
            super::SendTo::Collab(&collab_id)
        )?;
        ses.collab_session_guard = None;
        ses.cur_collab_id = None;
    }
    Ok(())
}


pub async fn msg_collab_report(data: &CollabReport, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(collab_id) = &ses.cur_collab_id {
        let ce = client_cmd!(CollabEvent, {
            paused: data.paused,
            r#loop: data.paused,
            seek_time_sec: data.seek_time_sec,
            from_user: ses.user_name.clone(),
            drawing: data.drawing.clone(),
        });
        server.emit_cmd(ce, super::SendTo::Collab(collab_id)).map(|_| ())
    } else {
        send_user_error!(&ses.user_id, server, Topic::None, "Report rejected: no active collab session.");
        return Ok(());
    }
}


pub async fn msg_move_to_folder(data: &proto::client::client_to_server_cmd::MoveToFolder, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(org) = &ses.organizer {
        let req = proto::org::MoveToFolderRequest {
            ses: Some(ses.org_session.clone()),
            dst_folder_id: data.dst_folder_id.clone(),
            ids: data.ids.clone(),
            listing_data: data.listing_data.clone(),
        };
        if let Err(e) = org.lock().await.move_to_folder(req).await {
            if e.code() == tonic::Code::Unimplemented {
                tracing::debug!("Organizer doesn't implement move_to_folder(). Ignoring.");
            } else {
                tracing::error!(err=?e, "Error in organizer move_to_folder() call");
                anyhow::bail!("Error in move_to_folder() organizer call: {:?}", e);
            }
        }
    } else { send_user_error!(&ses.user_id, server, Topic::None, "No organizer session."); }
    Ok(())
}

pub async fn msg_reorder_items(data: &ReorderItems, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(org) = &ses.organizer {
        let req = proto::org::ReorderItemsRequest {
            ses: Some(ses.org_session.clone()),
            ids: data.ids.clone(),
            listing_data: data.listing_data.clone(),
        };
        if let Err(e) = org.lock().await.reorder_items(req).await {
            if e.code() == tonic::Code::Unimplemented {
                tracing::debug!("Organizer doesn't implement reorder_items(). Ignoring.");
            } else {
                tracing::error!(err=?e, "Error in organizer reorder_items() call");
                anyhow::bail!("Error in reorder_items() organizer call: {:?}", e);
            }
        }
    } else { send_user_error!(&ses.user_id, server, Topic::None, "No organizer session."); }
    Ok(())
}


pub async fn msg_organizer_cmd(data: &proto::client::client_to_server_cmd::OrganizerCmd, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(org) = &ses.organizer {
        let req = proto::org::CmdFromClientRequest {
            ses: Some(ses.org_session.clone()),
            cmd: data.cmd.clone(),
            args: data.args.clone()
        };
        match org.lock().await.cmd_from_client(req).await {
            Err(e) => {
                tracing::error!(err=?e, "Error in organizer navigate_page() call");
                anyhow::bail!("Error in cmd_from_client() organizer call: {:?}", e);
            },
            Ok(res) => { return Ok(()); }
        }
    }
    Ok(())
}



#[derive(thiserror::Error, Debug)]
pub enum SessionClose {
    #[error("User logout")]
    Logout,
}

/// Dispatch a message from client to appropriate handler.
/// Return true if the session should be kept open, or false if it should be closed.
pub async fn msg_dispatch(req: &ClientToServerCmd, ses: &mut UserSession, server: &ServerState) -> Res<bool> {
    use proto::client::client_to_server_cmd::Cmd;
    let res = match req.cmd.as_ref() {
        None => {
            send_user_error!(&ses.user_id, server, Topic::None, format!("Missing command from client: {:?}", req));
            Ok(())
        }
        Some(cmd) => match cmd {
            Cmd::ListMyVideos(data) => msg_list_my_videos(&data, ses, server).await,
            Cmd::OpenVideo(data) => msg_open_video(&data, ses, server).await,
            Cmd::DelVideo(data) => msg_del_video(&data, ses, server).await,
            Cmd::RenameVideo(data) => msg_rename_video(&data, ses, server).await,
            Cmd::AddComment(data) => msg_add_comment(&data, ses, server).await,
            Cmd::EditComment(data) => msg_edit_comment(&data, ses, server).await,
            Cmd::DelComment(data) => msg_del_comment(&data, ses, server).await,
            Cmd::ListMyMessages(data) => msg_list_my_messages(&data, ses, server).await,
            Cmd::JoinCollab(data) => msg_join_collab(&data, ses, server).await,
            Cmd::LeaveCollab(data) => msg_leave_collab(&data, ses, server).await,
            Cmd::CollabReport(data) => msg_collab_report(&data, ses, server).await,
            Cmd::OrganizerCmd(data) => msg_organizer_cmd(&data, ses, server).await,
            Cmd::MoveToFolder(data) => msg_move_to_folder(&data, ses, server).await,
            Cmd::ReorderItems(data) => msg_reorder_items(&data, ses, server).await,
            Cmd::Logout(_) => {
                tracing::info!("logout from client: user={}", ses.user_id);
                return Err(SessionClose::Logout.into());
            },
        },
    };
    if let Err(e) = res {
        // Ignore authz errors, they are already logged
        if let None = e.downcast_ref::<user_session::AuthzError>() {
            let cmd_name = req.cmd.as_ref().map(|c| format!("{:?}", c)).unwrap_or_default();
            tracing::warn!("[{}] '{cmd_name}' failed: {}", ses.sid, e);
            send_user_error!(&ses.user_id, server, Topic::None, format!("{cmd_name} failed: {e}"));
        }
    }
    Ok(true)
}
