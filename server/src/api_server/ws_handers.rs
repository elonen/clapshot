#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::collections::HashMap;
use std::sync::{Arc};
use std::str::FromStr;
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

use super::user_session::{self, AuthzTopic};

use super::UserSession;

use crate::api_server::server_state::ServerState;
use crate::api_server::user_session::Topic;
use crate::database::error::DBError;
use crate::database::{models, DB, DbBasicQuery, DbQueryByUser, DbQueryByVideo, DBPaging};
use crate::database::schema::comments::drawing;
use crate::grpc::{db_comment_to_proto3, db_message_to_proto3, db_video_to_proto3};
use crate::{send_user_error, send_user_ok, client_cmd};

use lib_clapshot_grpc::proto;

use proto::org::authz_user_action_request as authz_req;


// ---------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------

/// Send user a list of all videos they have.
pub async fn msg_list_my_videos(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    ses.org_authz_with_default("list videos", true, server,
        true, AuthzTopic::Other(None, authz_req::other_op::Op::ViewHome)).await?;

    let videos = models::Video::get_by_user(&server.db, &ses.user_id, DBPaging::default())?;

    let h_txt = if videos.is_empty() {
        "<h2>You have no videos yet.</h2>"
    } else {
        "<h2>All your videos</h2>"
    };
    let heading = proto::PageItem{ item: Some(proto::page_item::Item::Html(h_txt.into()))};
    let listing = crate::grpc::folder_listing_for_videos(&videos, &server.url_base);
    let page = vec![heading, listing];

    server.emit_cmd(
        client_cmd!(ShowPage, {page_items: page}),
        super::SendTo::UserSession(&ses.sid))?;
    Ok(())
}

/// User opens a video.
/// Send them the video info and all comments related to it.
/// Register the session as a viewer of the video (video_session_guard).
pub async fn msg_open_video(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let video_id = data["id"].as_str().ok_or(anyhow!("id missing"))?;
    match models::Video::get(&server.db, &video_id.into()) {
        Err(DBError::NotFound()) => {
            send_user_error!(ses, server, Topic::Video(video_id), "No such video.");
        }
        Err(e) => { bail!(e); }
        Ok(v) => {
            ses.org_authz_with_default("open video", true, server,
                true, AuthzTopic::Video(&v, authz_req::video_op::Op::View)).await?;

            ses.video_session_guard = Some(server.link_session_to_video(video_id, ses.sender.clone()));

            let v = db_video_to_proto3(&v, &server.url_base);
            if v.playback_url.is_none() {
                return Err(anyhow!("No video file"));
            }

            server.emit_cmd(
                client_cmd!(OpenVideo, {video: Some(v)}),
                super::SendTo::UserSession(&ses.sid))?;

            let mut cmts = vec![];
            for mut c in models::Comment::get_by_video(&server.db, video_id, DBPaging::default())? {
                ses.fetch_drawing_data_into_comment(server, &mut c).await?;
                cmts.push(db_comment_to_proto3(&c));
            }

            server.emit_cmd(
                client_cmd!(AddComments, {comments: cmts}),
                super::SendTo::UserSession(&ses.sid))?;
        }
    }
    ses.cur_video_id = Some(video_id.into());
    Ok(())
}


pub async fn msg_del_video(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let video_id = data["id"].as_str().ok_or(anyhow!("id missing"))?;
    match models::Video::get(&server.db, &video_id.into()) {
        Ok(v) => {
            let default_perm = Some(ses.user_id.to_string()) == (&v).user_id || ses.user_id == "admin";
            ses.org_authz_with_default("delete video", true, server,
                default_perm, AuthzTopic::Video(&v, authz_req::video_op::Op::Delete)).await?;

            models::Video::delete(&server.db, &video_id.into())?;
            let mut details = format!("Added by '{}' ({}) on {}. Filename was {}.",
                v.user_name.clone().unwrap_or_default(),
                v.user_id.clone().unwrap_or_default(),
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
            if let Err(e) = move_video_to_trash(server, video_id) {
                details.push_str(&format!(" WARNING: Move to trash failed: {:?}.", e));
                cleanup_errors = true;
            }

            send_user_ok!(ses, &server, Topic::Video(video_id),
                if !cleanup_errors {"Video deleted."} else {"Video deleted, but cleanup had errors."},
                details, true);
        }
        Err(DBError::NotFound()) => {
            send_user_error!(ses, server, Topic::Video(video_id), "No such video. Cannot delete.");
        }
        Err(e) => { bail!(e); }
    }
    Ok(())
}

pub async fn msg_rename_video(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let video_id = data["id"].as_str().ok_or(anyhow!("id missing"))?;
    let new_name = data["new_name"].as_str().ok_or(anyhow!("new_name missing"))?;

    match models::Video::get(&server.db, &video_id.into()) {
        Ok(v) => {
            let default_perm = Some(ses.user_id.to_string()) == (&v).user_id || ses.user_id == "admin";
            ses.org_authz_with_default("rename video", true, server,
                default_perm, AuthzTopic::Video(&v, authz_req::video_op::Op::Rename)).await?;

            let new_name = new_name.trim();
            if new_name.is_empty() || !new_name.chars().any(|c| c.is_alphanumeric()) {
                send_user_error!(ses, server, Topic::Video(video_id), "Invalid video name (must have letters/numbers)");
                return Ok(());
            }
            if new_name.len() > 160 {
                send_user_error!(ses, server, Topic::Video(video_id), "Video name too long (max 160)");
                return Ok(());
            }
            models::Video::rename(&server.db, video_id, new_name)?;
            send_user_ok!(ses, server, Topic::Video(video_id), "Video renamed.",
                format!("New name: '{}'", new_name), true);
        }
        Err(DBError::NotFound()) => {
            send_user_error!(ses, server, Topic::Video(video_id), "No such video. Cannot rename.");
        }
        Err(e) => { bail!(e); }
    }
    Ok(())
}

pub async fn msg_add_comment(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let video_id = data["video_id"].as_str().ok_or(anyhow!("video_id missing"))?;

    match models::Video::get(&server.db, &video_id.into()) {
        Ok(v) => {
            let default_perm = Some(ses.user_id.to_string()) == (&v).user_id || ses.user_id == "admin";
            ses.org_authz_with_default("comment video", true, server,
                default_perm, AuthzTopic::Video(&v, authz_req::video_op::Op::Comment)).await?;
        },
        Err(DBError::NotFound()) => {
            send_user_error!(ses, server, Topic::Video(video_id), "No such video. Cannot comment.");
            return Ok(());
        }
        Err(e) => { bail!(e); }
    }

    // Parse drawing data if present and write to file
    let mut drwn = data["drawing"].as_str().map(|s| s.to_string());
    if let Some(d) = drwn.clone() {
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

    let parent_id = match data["parent_id"].as_str().map(|s| s.parse::<i32>()) {
        Some(Ok(id)) => Some(id),
        Some(Err(_)) => { bail!("Invalid parent_id for comment"); }
        None => None,
    };

    let c = models::CommentInsert {
        video_id: video_id.to_string(),
        parent_id,
        user_id: ses.user_id.clone(),
        user_name: ses.user_name.clone(),
        comment: data["comment"].as_str().ok_or(anyhow!("comment missing"))?.to_string(),
        timecode: data["timecode"].as_str().map(String::from),
        drawing: drwn,
    };
    let c = models::Comment::add(&server.db, &c)
        .map_err(|e| anyhow!("Failed to add comment: {:?}", e))?;
    // Send to all clients watching this video
    ses.emit_new_comment(server, c, super::SendTo::VideoId(&video_id)).await?;
    Ok(())
}


pub async fn msg_edit_comment(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let id = i32::from_str(&data["id"].as_str().ok_or(anyhow!("id missing"))?)?;
    let new_text = data["comment"].as_str().ok_or(anyhow!("comment missing"))?.to_string();

    match models::Comment::get(&server.db, &id) {
        Ok(old) => {
            let default_perm = ses.user_id == old.user_id || ses.user_id == "admin";
            ses.org_authz_with_default("edit comment", true, server,
                default_perm, AuthzTopic::Comment(&old, authz_req::comment_op::Op::Edit)).await?;

            let vid = &old.video_id;
            models::Comment::edit(&server.db, id, &new_text)?;

            server.emit_cmd(
                client_cmd!(DelComment, {comment_id: id.to_string()}),
                super::SendTo::VideoId(&vid))?;

            let c = models::Comment::get(&server.db, &id)?;
            ses.emit_new_comment(server, c, super::SendTo::VideoId(&vid)).await?;
        }
        Err(DBError::NotFound()) => {
            send_user_error!(ses, server, Topic::None, "Failed to edit comment.", "No such comment. Cannot edit.", true);
        }
        Err(e) => { bail!(e); }
    }
    Ok(())
}


pub async fn msg_del_comment(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let id = i32::from_str(&data["id"].as_str().ok_or(anyhow!("id missing"))?)?;
    match models::Comment::get(&server.db, &id) {
        Ok(cmt) => {
            let default_perm = ses.user_id == cmt.user_id || ses.user_id == "admin";
            ses.org_authz_with_default("delete comment", true, server,
                default_perm, AuthzTopic::Comment(&cmt, authz_req::comment_op::Op::Delete)).await?;

            let vid = cmt.video_id;
            if ses.user_id != cmt.user_id && ses.user_id != "admin" {
                send_user_error!(ses, server, Topic::Video(&vid), "Failed to delete comment.", "You can only delete your own comments", true);
                return Ok(());
            }
            let all_comm = models::Comment::get_by_video(&server.db, &vid, DBPaging::default())?;
            if all_comm.iter().any(|c| c.parent_id.map(|i| i.to_string()) == Some(id.to_string())) {
                send_user_error!(ses, server, Topic::Video(&vid), "Failed to delete comment.", "Comment has replies. Cannot delete.", true);
                return Ok(());
            }
            models::Comment::delete(&server.db, &id)?;
            server.emit_cmd(
                client_cmd!(DelComment, {comment_id: id.to_string()}),
                super::SendTo::VideoId(&vid))?;
        }
        Err(DBError::NotFound()) => {
            send_user_error!(ses, server, Topic::None, "Failed to delete comment.", "No such comment. Cannot delete.", true);
        }
        Err(e) => { bail!(e); }
    }
    Ok(())
}

pub async fn msg_list_my_messages(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let msgs = models::Message::get_by_user(&server.db, &ses.user_id, DBPaging::default())?;
    server.emit_cmd(
        client_cmd!(ShowMessages, { msgs: (&msgs).into_iter().map(|m| db_message_to_proto3(&m)).collect() }),
        super::SendTo::UserSession(&ses.sid)
    )?;
    for m in msgs {
        if !m.seen { models::Message::set_seen(&server.db, m.id, true)?; }
    }
    Ok(())
}

pub async fn msg_join_collab(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let collab_id = data["collab_id"].as_str().ok_or(anyhow!("collab_id missing"))?;
    let video_id = data["video_id"].as_str().ok_or(anyhow!("video_id missing"))?;

    if let Some(collab_id) = ses.cur_collab_id.clone() {
        if server.sender_is_collab_participant(collab_id.as_str(), &ses.sender) {
            tracing::debug!("{} is already in collab {}. Ignoring double join.", ses.user_name, collab_id);
            return Ok(());
        }
    }

    ses.collab_session_guard = None;
    ses.cur_collab_id = None;

    match models::Video::get(&server.db, &video_id.into()) {
        Err(DBError::NotFound()) => {
            send_user_error!(ses, server, Topic::Video(video_id), "No such video.");
        }
        Err(e) => { bail!(e); }
        Ok(v) => {
            ses.org_authz_with_default("join collab", true, server,
                true, AuthzTopic::Other(Some(collab_id.clone()), authz_req::other_op::Op::JoinCollabSession)).await?;

            match server.link_session_to_collab(collab_id, video_id, ses.sender.clone()) {
                Ok(csg) => {
                    ses.collab_session_guard = Some(csg);
                    ses.cur_collab_id = Some(collab_id.to_string());
                    server.emit_cmd(
                        client_cmd!(ShowMessages, { msgs: vec![
                                proto::UserMessage {
                                r#type: proto::user_message::Type::Ok as i32,
                                message: format!("'{}' joined collab", &ses.user_name),
                                ..Default::default()
                            }]
                        }),
                        super::SendTo::Collab(&collab_id)
                    )?;
                }
                Err(e) => {
                    send_user_error!(ses, server, Topic::Video(video_id), format!("Failed to join collab session: {}", e));
                }
            }
        }
    }
    Ok(())
}

pub async fn msg_leave_collab(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
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

pub async fn msg_collab_report(data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(collab_id ) = &ses.cur_collab_id {
        let paused = data["paused"].as_bool().ok_or(anyhow!("paused missing"))?;
        let seek_time = data["seek_time"].as_f64().ok_or(anyhow!("seek_time missing"))?;
        let img_url = data["drawing"].as_str();
        let ce = if img_url.is_some() {
            client_cmd!(CollabEvent, {
                paused: paused,
                seek_time_sec: seek_time,
                from_user: ses.user_name.clone(),
                drawing: img_url.map(|s| s.to_string()),
            })
        } else {
            client_cmd!(CollabEvent, {
                paused: paused,
                seek_time_sec: seek_time,
                from_user: ses.user_name.clone(),
                drawing: None,
            })
        };
        server.emit_cmd(ce, super::SendTo::Collab(collab_id)).map(|_| ())
    } else {
        send_user_error!(ses, server, Topic::None, "Report rejected: no active collab session.");
        return Ok(());
    }
}

/// Dispatch a message to the appropriate handler.
/// Returns false if session should be closed.
pub async fn msg_dispatch(cmd: &str, data: &serde_json::Value, ses: &mut UserSession, server: &ServerState) -> Res<bool> {
    let res = match cmd {
        "list_my_videos" => msg_list_my_videos(data, ses, server).await,
        "open_video" => msg_open_video(data, ses, server).await,
        "del_video" => msg_del_video(data, ses, server).await,
        "rename_video" => msg_rename_video(data, ses, server).await,
        "add_comment" => msg_add_comment(data, ses, server).await,
        "edit_comment" => msg_edit_comment(data, ses, server).await,
        "del_comment" => msg_del_comment(data, ses, server).await,
        "list_my_messages" => msg_list_my_messages(data, ses, server).await,
        "join_collab" => msg_join_collab(data, ses, server).await,
        "leave_collab" => msg_leave_collab(data, ses, server).await,
        "collab_report" => msg_collab_report(data, ses, server).await,
        "logout" => {
            tracing::info!("logout: user={}", ses.user_id);
            return Ok(false);
        },
        "echo" => {
            let answ = format!("Echo: {}", data.as_str().ok_or(anyhow!("data not found"))?);
            ses.sender.send(WsMsg::text(answ))?;
            Ok(())
        },
        _ => {
            send_user_error!(ses, server, Topic::None, format!("Unknown command: '{}'", cmd));
            Ok(())
        }
    };

    if let Err(e) = res {
        // Ignore authz errors, they are already logged
        if let None = e.downcast_ref::<user_session::AuthzError>() {
            tracing::warn!("[{}] '{cmd}' failed: {}", ses.sid, e);
            send_user_error!(ses, server, Topic::None, format!("{cmd} failed: {e}"));
        }
    }
    Ok(true)
}
