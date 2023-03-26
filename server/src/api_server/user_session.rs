use std::sync::Arc;
use crate::database::models;

use super::{WsMsgSender, server_state::ServerState, SendTo};
use base64::{Engine as _, engine::general_purpose as Base64GP};

type Res<T> = anyhow::Result<T>;

pub enum Topic<'a> {
    Video(&'a str),
    Comment(i32),
    None
}

#[macro_export]
macro_rules! send_user_msg(
    ($event_name:expr, $ses:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => {
        let (comment_id, video_hash) = match $topic {
            Topic::Video(video_hash) => (None, Some(video_hash.into())),
            Topic::Comment(comment_id) => (Some(comment_id.into()), None),
            Topic::None => (None, None)
        };
        $server.push_notify_message(&models::MessageInsert {
            event_name: $event_name.into(),
            user_id: $ses.user_id.clone(),
            ref_comment_id: comment_id,
            seen: false,
            ref_video_hash: video_hash,
            message: $msg.into(),
            details: $details.into()
        }, crate::api_server::SendTo::UserId(&($ses.user_id)), $persist)?;
    };
    ($event_name:expr, $ses:expr, $server:expr, $topic:expr, $msg:expr, $persist:expr) => {
        send_user_error!($ses, $server, $topic, $msg, String::new(), $persist)
    };
    ($event_name:expr, $ses:expr, $server:expr, $topic:expr, $msg:expr) => {
        send_user_error!($ses, $server, $topic, $msg, String::new(), false)
    };
);

#[macro_export]
macro_rules! send_user_error(
    ($ses:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => { crate::send_user_msg!("error", $ses, $server, $topic, $msg, $details, $persist); };
    ($ses:expr, $server:expr, $topic:expr, $msg:expr, $persist:expr) => { send_user_error!($ses, $server, $topic, $msg, String::new(), $persist); };
    ($ses:expr, $server:expr, $topic:expr, $msg:expr) => { send_user_error!($ses, $server, $topic, $msg, String::new(), false); };
);

#[macro_export]
macro_rules! send_user_ok(
    ($ses:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => { crate::send_user_msg!("ok", $ses, $server, $topic, $msg, $details, $persist); };
    ($ses:expr, $server:expr, $topic:expr, $msg:expr, $persist:expr) => { send_user_ok!($ses, $server, $topic, $msg, String::new(), $persist); };
    ($ses:expr, $server:expr, $topic:expr, $msg:expr) => { send_user_ok!($ses, $server, $topic, $msg, String::new(), false); };
);



pub type OpaqueGuard = Arc<tokio::sync::Mutex<dyn Send>>;

#[derive (Clone)]
pub struct UserSession {
    pub sid: String,
    pub sender: WsMsgSender,
    pub user_id: String,
    pub user_name: String,
    pub cur_video_hash: Option<String>,
    pub cur_collab_id: Option<String>,
    pub video_session_guard: Option<OpaqueGuard>,
    pub collab_session_guard: Option<OpaqueGuard>,
}

impl UserSession {

    pub async fn emit_new_comment(&self, server: &ServerState, mut c: models::Comment, send_to: SendTo<'_>) -> Res<()> {
        if let Some(drawing) = &mut c.drawing {
            if drawing != "" { 
                // If drawing is present, read it from disk and encode it into a data URI.
                if !drawing.starts_with("data:") {
                    let path = server.videos_dir.join(&c.video_hash).join("drawings").join(&drawing);
                    if path.exists() {
                        let data = tokio::fs::read(path).await?;
                        *drawing = format!("data:image/webp;base64,{}", Base64GP::STANDARD_NO_PAD.encode(&data));
                    } else {
                        tracing::warn!("Drawing file not found for comment: {}", c.id);
                        c.comment += " [DRAWING NOT FOUND]";
                    }
                } else {
                    // If drawing is already a data URI, just use it as is.
                    // This shouldn't happen anymore, but it's here just in case.
                    tracing::warn!("Comment '{}' has data URI drawing stored in DB. Should be on disk.", c.id);
                }
            }
        }
        let mut fields = c.to_json()?;
        fields["comment_id"] = fields["id"].take();  // swap id with comment_id, because the client expects comment_id        
        server.emit_cmd("new_comment", &fields , send_to).map(|_| ())
    }

}
