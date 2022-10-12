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

use super::WsSessionArgs;

use crate::database::error::DBError;
use crate::database::models;


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
            ses.push_notify_message(&models::MessageInsert {
                event_name: "error".into(),
                user_id: ses.user_id.into(),
                ref_comment_id: None,
                seen: false,
                ref_video_hash: Some(video_hash.into()),
                message: "No such video.".into(),
                details: String::new()
            }, false)?;
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



pub async fn msg_dispatch(cmd: &str, data: &serde_json::Value, ses: &mut WsSessionArgs<'_>) -> Res<()> {
    match cmd {
        "list_my_videos" => msg_list_my_videos(data, ses).await,
        "open_video" => msg_open_video(data, ses).await,
        //"del_video" => msg_del_video(data, ses).await,
        //"add_comment" => msg_add_comment(data, ses).await,
        //"edit_comment" => msg_edit_comment(data, ses).await,
        //"del_comment" => msg_del_comment(data, ses).await,
        //"list_my_messages" => msg_list_my_messages(data, ses).await,
        //"logout" => msg_logout(data, ses).await,
        "echo" => {
            let answ = format!("Echo: {}", data.as_str().ok_or("data not found")?);
            ses.sender.send(WsMsg::text(answ))?;
            Ok(())
        },
        _ => {
            let answ = format!("Unknown command: '{}'", cmd);
            tracing::warn!("[{}] {}", ses.sid, answ);
            ses.sender.send(WsMsg::text(answ))?;
            Ok(())
        }
    }
}
