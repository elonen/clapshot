#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
type WsMsg = warp::ws::Message;

type Res<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type MsgSender = tokio::sync::mpsc::UnboundedSender<WsMsg>;
type SenderList = Vec<MsgSender>;
type SenderListMap = Arc<RwLock<HashMap<String, SenderList>>>;

use serde_json::json;

use data_url::{DataUrl, mime};
use sha2::{Sha256, Digest};
use hex;

use super::WsSessionArgs;

use crate::database::error::DBError;
use crate::database::{models, DB};
use crate::database::schema::comments::drawing;


// ---------------------------------------------------------------------
// Result sending helpers
// ---------------------------------------------------------------------

enum Topic<'a> {
    Video(&'a str),
    Comment(i32),
    None
}

macro_rules! send_user_msg(
    ($event_name:expr, $ses:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => {
        let (comment_id, video_hash) = match $topic {
            Topic::Video(video_hash) => (None, Some(video_hash.into())),
            Topic::Comment(comment_id) => (Some(comment_id.into()), None),
            Topic::None => (None, None)
        };
        $ses.push_notify_message(&models::MessageInsert {
            event_name: $event_name.into(),
            user_id: $ses.user_id.into(),
            ref_comment_id: comment_id,
            seen: false,
            ref_video_hash: video_hash,
            message: $msg.into(),
            details: $details.into()
        }, $persist)?;
    };
    ($event_name:expr, $ses:expr, $topic:expr, $msg:expr, $persist:expr) => {
        send_user_error!($ses, $topic, $msg, String::new(), $persist)
    };
    ($event_name:expr, $ses:expr, $topic:expr, $msg:expr) => {
        send_user_error!($ses, $topic, $msg, String::new(), false)
    };
);

macro_rules! send_user_error(
    ($ses:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => { send_user_msg!("error", $ses, $topic, $msg, $details, $persist); };
    ($ses:expr, $topic:expr, $msg:expr, $persist:expr) => { send_user_error!($ses, $topic, $msg, String::new(), $persist); };
    ($ses:expr, $topic:expr, $msg:expr) => { send_user_error!($ses, $topic, $msg, String::new(), false); };
);

macro_rules! send_user_ok(
    ($ses:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => { send_user_msg!("ok", $ses, $topic, $msg, $details, $persist); };
    ($ses:expr, $topic:expr, $msg:expr, $persist:expr) => { send_user_ok!($ses, $topic, $msg, String::new(), $persist); };
    ($ses:expr, $topic:expr, $msg:expr) => { send_user_ok!($ses, $topic, $msg, String::new(), false); };
);

// ---------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------

/// Send user a list of all videos they have.
pub async fn msg_list_my_videos(data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    let videos = ses.server.db.get_all_user_videos(&ses.user_id)?;
    ses.emit_cmd("user_videos", &json!({
            "username": ses.user_name.clone(),
            "user_id": ses.user_id.clone(),
            "videos": videos }),
        super::SendTo::CurSession())?;
    Ok(())
}

/// User opens a video.
/// Send them the video info and all comments related to it.
/// Register the session as a viewer of the video (video_session_guard).
pub async fn msg_open_video(data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    let video_hash = data["video_hash"].as_str().ok_or("video_hash missing")?;
    match ses.server.db.get_video(video_hash) {
        Err(DBError::NotFound()) => {
            send_user_error!(ses, Topic::Video(video_hash), "No such video.");
        }
        Err(e) => {
            return Err(e.into());
        }
        Ok(v) => {
            ses.video_session_guard = Some(ses.server.link_session_to_video(video_hash, ses.sender.clone()));
            let fields = serde_json::to_value(&v)?;
            ses.emit_cmd("open_video", &fields, super::SendTo::CurSession() )?;
            for c in ses.server.db.get_video_comments(video_hash)? {
                ses.emit_new_comment(c, super::SendTo::CurSession()).await?;
            }
        }
    }
    Ok(())
}

pub async fn msg_del_video(data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    let video_hash = data["video_hash"].as_str().ok_or("video_hash missing")?;
    match ses.server.db.get_video(video_hash) {
        Ok(v) => {
            if Some(ses.user_id.to_string()) != v.added_by_userid && ses.user_id != "admin" {
                send_user_error!(ses, Topic::Video(video_hash), "Video not owned by you. Cannot delete.");
            } else {
                ses.server.db.del_video_and_comments(video_hash)?;
                let details = format!("Added by {:?} ({:?}) on {}. Filename was '{:?}'",
                    v.added_by_username, v.added_by_userid, v.added_time, v.orig_filename);
                send_user_ok!(ses, Topic::Video(video_hash), "Video deleted.", details, true);
            }
        }
        Err(DBError::NotFound()) => {
            send_user_error!(ses, Topic::Video(video_hash), "No such video. Cannot delete.");
        }
        Err(e) => { return Err(e.into()); }
    }
    Ok(())
}

pub async fn msg_add_comment(data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    let vh = data["video_hash"].as_str().ok_or("video_hash missing")?.to_string();

    if let Err(DBError::NotFound())  = ses.server.db.get_video(&vh) {
        send_user_error!(ses, Topic::Video(&vh), "No such video. Cannot comment.");
        return Ok(());
    }

    // Parse drawing data if present and write to file
    let mut drwn = data["drawing"].as_str().map(|s| s.to_string());
    if let Some(d) = drwn.clone() {
        if d.starts_with("data:") {

            // Convert data URI to bytes
            let img_uri = DataUrl::process(&d)
                .map_err(|e| format!("Invalid drawing data URI"))?;
            
            if img_uri.mime_type().type_ != "image" || img_uri.mime_type().subtype != "webp" {
                return Err(format!("Invalid mimetype in drawing: {:?}", img_uri.mime_type()).into());
            }
            let img_data = img_uri.decode_to_vec().map_err(|e| format!("Failed to decode drawing data URI: {:?}", e))?;

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
            let drawing_path = ses.server.videos_dir.join(&vh).join("drawings").join(&fname);
            std::fs::create_dir_all(drawing_path.parent().unwrap())
                .map_err(|e| format!("Failed to create drawings dir: {:?}", e))?;
            async_std::fs::write(drawing_path, img_data.0).await.map_err(
                |e| format!("Failed to write drawing file: {:?}", e))?;
            
            // Replace data URI with filename
            drwn = Some(fname);
        }
    };

    let c = models::CommentInsert {
        video_hash: vh.clone(),
        parent_id: data["parent_id"].as_i64().map(|x| x as i32),
        user_id: ses.user_id.into(),
        username: ses.user_name.into(),
        comment: data["comment"].as_str().ok_or("comment missing")?.to_string(),
        timecode: data["timecode"].as_str().map(String::from),
        drawing: drwn,
    };
    let new_id = ses.server.db.add_comment(&c)
        .map_err(|e| format!("Failed to add comment: {:?}", e))?;
    let c = ses.server.db.get_comment(new_id)?;

    // Send to all clients watching this video
    ses.emit_new_comment(c, super::SendTo::VideoHash(&vh)).await?;
    Ok(())
}

pub async fn msg_edit_comment(data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    let comment_id = data["comment_id"].as_i64().ok_or("comment_id missing")? as i32;
    let new_text = data["comment"].as_str().ok_or("comment missing")?.to_string();

    match ses.server.db.get_comment(comment_id) {
        Ok(old) => {
            let vh = old.video_hash;
            if ses.user_id != old.user_id && ses.user_id != "admin" {
                send_user_error!(ses, Topic::Video(&vh), "You can only edit your own comments");
                return Ok(());
            }
            ses.server.db.edit_comment(comment_id, &new_text)?;
            ses.emit_cmd("del_comment", &json!({ "comment_id": comment_id }), super::SendTo::VideoHash(&vh))?;
            let c = ses.server.db.get_comment(comment_id)?;
            ses.emit_new_comment(c, super::SendTo::VideoHash(&vh)).await?;
        }
        Err(DBError::NotFound()) => {
            send_user_error!(ses, Topic::None, "No such comment. Cannot edit.");
        }
        Err(e) => { return Err(e.into()); }
    }
    Ok(())
}

pub async fn msg_del_comment(data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    let comment_id = data["comment_id"].as_i64().ok_or("comment_id missing")? as i32;

    match ses.server.db.get_comment(comment_id) {
        Ok(cmt) => {
            let vh = cmt.video_hash;
            if ses.user_id != cmt.user_id && ses.user_id != "admin" {
                send_user_error!(ses, Topic::Video(&vh), "You can only delete your own comments");
                return Ok(());
            }
            let all_comm = ses.server.db.get_video_comments(&vh)?;
            if all_comm.iter().any(|c| c.parent_id == Some(comment_id)) {
                send_user_error!(ses, Topic::Video(&vh), "Comment has replies. Cannot delete.");
                return Ok(());
            }
            ses.server.db.del_comment(comment_id)?;
            ses.emit_cmd("del_comment", &json!({ "comment_id": comment_id }), super::SendTo::VideoHash(&vh))?;
        }
        Err(DBError::NotFound()) => {
            send_user_error!(ses, Topic::None, "No such comment. Cannot delete.");
        }
        Err(e) => { return Err(e.into()); }
    }
    Ok(())
}

pub async fn msg_list_my_messages(data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    let msgs = ses.server.db.get_user_messages(&ses.user_id)?;
    for m in msgs {
        ses.emit_cmd("message", &serde_json::to_value(&m)?, super::SendTo::CurSession())?;
        if !m.seen {
            ses.server.db.set_message_seen(m.id, true)?;
        }
    }
    Ok(())
}

pub async fn msg_logout(data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    tracing::info!("logout: user={}", ses.user_id);
    drop(ses.sender);
    Ok(())
}


pub async fn msg_dispatch(cmd: &str, data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    let res = match cmd {
        "list_my_videos" => msg_list_my_videos(data, ses).await,
        "open_video" => msg_open_video(data, ses).await,
        "del_video" => msg_del_video(data, ses).await,
        "add_comment" => msg_add_comment(data, ses).await,
        "edit_comment" => msg_edit_comment(data, ses).await,
        "del_comment" => msg_del_comment(data, ses).await,
        "list_my_messages" => msg_list_my_messages(data, ses).await,
        "logout" => msg_logout(data, ses).await,
        "echo" => {
            let answ = format!("Echo: {}", data.as_str().ok_or("data not found")?);
            ses.sender.send(WsMsg::text(answ))?;
            Ok(())
        },
        _ => {
            send_user_error!(ses, Topic::None, format!("Unknown command: '{}'", cmd));
            Ok(())
        }
    };
    if let Err(e) = res {
        tracing::warn!("[{}] '{cmd}' failed: {}", ses.sid, e);
        send_user_error!(ses, Topic::None, format!("{cmd} failed: {e}"));
    }
    Ok(())
}
