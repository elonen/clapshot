use std::sync::Arc;
use crate::{database::models::{self, Video, Comment}, grpc::{grpc_client::OrganizerConnection, db_video_to_proto3, db_comment_to_proto3}, client_cmd};

use super::{WsMsgSender, server_state::ServerState, SendTo};
use base64::{Engine as _, engine::general_purpose as Base64GP};
use lib_clapshot_grpc::proto;
use tracing::{debug, error};

type Res<T> = anyhow::Result<T>;

pub enum Topic<'a> {
    Video(&'a str),
    Comment(i32),
    None
}

#[macro_export]
macro_rules! send_user_msg(
    ($msg_type:expr, $ses:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => {
        let (comment_id, video_hash) = match $topic {
            Topic::Video(video_hash) => (None, Some(video_hash.into())),
            Topic::Comment(comment_id) => (Some(comment_id.into()), None),
            Topic::None => (None, None)
        };
        $server.push_notify_message(&models::MessageInsert {
            event_name: crate::database::models::proto_msg_type_to_event_name($msg_type).to_string(),
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
    ($ses:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => { crate::send_user_msg!(proto::user_message::Type::Error, $ses, $server, $topic, $msg, $details, $persist); };
    ($ses:expr, $server:expr, $topic:expr, $msg:expr, $persist:expr) => { send_user_error!($ses, $server, $topic, $msg, String::new(), $persist); };
    ($ses:expr, $server:expr, $topic:expr, $msg:expr) => { send_user_error!($ses, $server, $topic, $msg, String::new(), false); };
);

#[macro_export]
macro_rules! send_user_ok(
    ($ses:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => { crate::send_user_msg!(proto::user_message::Type::Ok, $ses, $server, $topic, $msg, $details, $persist); };
    ($ses:expr, $server:expr, $topic:expr, $msg:expr, $persist:expr) => { send_user_ok!($ses, $server, $topic, $msg, String::new(), $persist); };
    ($ses:expr, $server:expr, $topic:expr, $msg:expr) => { send_user_ok!($ses, $server, $topic, $msg, String::new(), false); };
);

#[derive (Debug, Clone)]
pub enum AuthzTopic<'a> {
    Video(&'a Video, proto::authz_user_action_request::video_op::Op),
    Comment(&'a Comment, proto::authz_user_action_request::comment_op::Op),
    Other(Option<&'a str>, proto::authz_user_action_request::other_op::Op)
}

#[derive (thiserror::Error, Debug)]
pub enum AuthzError {
    #[error("Permission denied")]
    Denied,
}


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

    pub organizer: Option<Arc<tokio::sync::Mutex<OrganizerConnection>>>,
    pub org_session: proto::UserSessionData,
}

impl UserSession {

    /// Reads the drawing data from disk and encodes it into a data URI, updating the comment's drawing field
    pub async fn fetch_drawing_data_into_comment(&self, server: &ServerState, c: &mut models::Comment) -> Res<()> {
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
        };
        Ok(())
    }

    pub async fn emit_new_comment(&self, server: &ServerState, mut c: models::Comment, send_to: SendTo<'_>) -> Res<()> {
        self.fetch_drawing_data_into_comment(server, &mut c).await?;
        let cmd = client_cmd!(AddComments, {comments: vec![db_comment_to_proto3(&c)]});
        server.emit_cmd(cmd, send_to).map(|_| ())
    }

    fn try_send_error<'a>(&self, server: &ServerState, msg: String, details: Option<String>, op: &AuthzTopic<'a>) -> anyhow::Result<()> {
        let topic = match op {
            AuthzTopic::Video(v, _op) => Topic::Video(&v.video_hash),
            AuthzTopic::Comment(c, _op) => Topic::Comment(c.id),
            AuthzTopic::Other(_t, _op) => Topic::None,
        };
        if let Some(details) = details {
            send_user_error!(self, server, topic, msg, details, true);
        } else {
            send_user_error!(self, server, topic, msg);
        }
        Ok(())
    }

    /// Check from Organizer if the user is allowed to perform given action.
    ///
    /// Some(true) = allowed
    /// Some(false) = denied
    /// None = default, as determined by the server - no Organizer or it doesn't support authz
    ///
    /// If Organizer is not connected, returns None.
    /// If check fails and Organizer is connected, logs an error and denies the action.
    /// If the user is not allowed, an error message is sent to the user if `msg_on_deny` is true.
    pub async fn org_authz<'a>(
        &self,
        desc: &str,
        msg_on_deny: bool,
        server: &ServerState,
        op: AuthzTopic<'a>,
    ) -> Option<bool> {
        let org = match &self.organizer {
            Some(org) => org,
            None => { return None; }
        };
        tracing::debug!(op=?op, user=self.user_id, desc, "Checking authz from Organizer");
        let pop = match op {
            AuthzTopic::Video(v, op) => proto::authz_user_action_request::Op::VideoOp(
                proto::authz_user_action_request::VideoOp {
                    op: op.into(),
                    video: Some(db_video_to_proto3(v, &server.url_base)) }),
            AuthzTopic::Comment(c, op) => proto::authz_user_action_request::Op::CommentOp(
                proto::authz_user_action_request::CommentOp {
                    op: op.into(),
                    comment: Some(db_comment_to_proto3(c)) }),
            AuthzTopic::Other(subj, op) => proto::authz_user_action_request::Op::OtherOp(
                proto::authz_user_action_request::OtherOp {
                    op: op.into(),
                    subject: subj.map(|s| s.into()) }),
        };
        let req = proto::AuthzUserActionRequest { ses: Some(self.org_session.clone()), op: Some(pop) };
        let res = org.lock().await.authz_user_action(req).await;
        match res {
            Err(e) => {
                if e.code() == tonic::Code::Unimplemented {
                    tracing::debug!(desc, user=self.user_id, "Organizer doesn't support authz");
                    None
                } else {
                    error!(desc, user=self.user_id, err=?e, "Error while authorizing user action");
                    self.try_send_error(&server, format!("Internal error in authz: {}", desc), None, &op).ok();
                    Some(false)
                }
            },
            Ok(res) => {
                match res.get_ref().is_authorized {
                    Some(false) => {
                        let msg = res.get_ref().message.clone().map(|s| s).unwrap_or_else(|| "Permission denied".to_string());
                        let details = res.get_ref().details.clone();
                        if msg_on_deny { self.try_send_error(&server, msg, details, &op).ok(); }
                        debug!(desc, user=self.user_id, "Organizer said: Permission denied");
                        Some(false)
                    },
                    Some(true) => {
                        debug!(desc, user=self.user_id, "Organizer said: Authorized OK");
                        Some(true)
                    },
                    None => {
                        debug!(desc, user=self.user_id, "Organizer said: I don't authz, use defaults");
                        None
                    }
                }
            }
        }
    }

    pub async fn org_authz_with_default<'a>(
        &self,
        desc: &str,
        msg_on_deny: bool,
        server: &ServerState,
        default: bool,
        op: AuthzTopic<'a>,
    ) -> Result<(), AuthzError> {
        if let Some(res) = self.org_authz(desc, msg_on_deny, server, op.clone()).await {
            if res { Ok(()) } else { Err(AuthzError::Denied) }
        } else {
            if default { Ok(()) } else {
                if msg_on_deny {
                    self.try_send_error(&server, format!("Permission denied: {}", desc), Some(format!("{:?}", &op)), &op).ok();
                };
                Err(AuthzError::Denied)
            }
        }
    }
}
