use std::sync::Arc;
use crate::{database::models::{self, MediaFile, Comment}, grpc::grpc_client::OrganizerConnection, client_cmd};

use super::{WsMsgSender, server_state::ServerState, SendTo};
use lib_clapshot_grpc::proto;
use tracing::{debug, error};

type Res<T> = anyhow::Result<T>;

pub enum Topic<'a> {
    MediaFile(&'a str),
    Comment(i32),
    None
}

#[macro_export]
macro_rules! send_user_msg(
    ($msg_type:expr, $user_id:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => {
        let (comment_id, media_file_id) = match $topic {
            Topic::MediaFile(media_file_id) => (None, Some(media_file_id.into())),
            Topic::Comment(comment_id) => (Some(comment_id.into()), None),
            Topic::None => (None, None)
        };
        use crate::grpc::db_models::proto_msg_type_to_event_name;
        $server.push_notify_message(&models::MessageInsert {
            event_name: proto_msg_type_to_event_name($msg_type).to_string(),
            user_id: $user_id.clone(),
            comment_id,
            seen: false,
            media_file_id,
            message: $msg.into(),
            details: $details.into(),
            subtitle_id: None,
        }, crate::api_server::SendTo::UserId(&$user_id), $persist)?;
    };
    ($event_name:expr, $user_id:expr, $server:expr, $topic:expr, $msg:expr, $persist:expr) => {
        send_user_error!($user_id, $server, $topic, $msg, String::new(), $persist)
    };
    ($event_name:expr, $user_id:expr, $server:expr, $topic:expr, $msg:expr) => {
        send_user_error!($user_id, $server, $topic, $msg, String::new(), false)
    };
);

#[macro_export]
macro_rules! send_user_error(
    ($user_id:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => { crate::send_user_msg!(proto::user_message::Type::Error, $user_id, $server, $topic, $msg, $details, $persist); };
    ($user_id:expr, $server:expr, $topic:expr, $msg:expr, $persist:expr) => { send_user_error!($user_id, $server, $topic, $msg, String::new(), $persist); };
    ($user_id:expr, $server:expr, $topic:expr, $msg:expr) => { send_user_error!($user_id, $server, $topic, $msg, String::new(), false); };
);

#[macro_export]
macro_rules! send_user_ok(
    ($user_id:expr, $server:expr, $topic:expr, $msg:expr, $details:expr, $persist:expr) => { crate::send_user_msg!(proto::user_message::Type::Ok, $user_id, $server, $topic, $msg, $details, $persist); };
    ($user_id:expr, $server:expr, $topic:expr, $msg:expr, $persist:expr) => { send_user_ok!($user_id, $server, $topic, $msg, String::new(), $persist); };
    ($user_id:expr, $server:expr, $topic:expr, $msg:expr) => { send_user_ok!($user_id, $server, $topic, $msg, String::new(), false); };
);

#[derive (Debug, Clone)]
pub enum AuthzTopic<'a> {
    MediaFile(&'a MediaFile, proto::org::authz_user_action_request::media_file_op::Op),
    Comment(&'a Comment, proto::org::authz_user_action_request::comment_op::Op),
    Other(Option<&'a str>, proto::org::authz_user_action_request::other_op::Op)
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
    pub is_admin: bool,

    pub cur_media_file_id: Option<String>,
    pub cur_collab_id: Option<String>,
    pub media_session_guard: Option<OpaqueGuard>,
    pub collab_session_guard: Option<OpaqueGuard>,

    pub organizer: Option<Arc<tokio::sync::Mutex<OrganizerConnection>>>,
    pub org_session: proto::org::UserSessionData,
}

impl UserSession {

    pub async fn emit_new_comment(&self, server: &ServerState, mut c: models::Comment, send_to: SendTo<'_>) -> Res<()> {
        server.fetch_drawing_data_into_comment(&mut c).await?;
        let cmd = client_cmd!(AddComments, {comments: vec![c.to_proto3()]});
        server.emit_cmd(cmd, send_to).map(|_| ())
    }
}


fn try_send_error<'a>(user_id: &str, server: &ServerState, msg: String, details: Option<String>, op: &AuthzTopic<'a>) -> anyhow::Result<()> {
    let topic = match op {
        AuthzTopic::MediaFile(v, _op) => Topic::MediaFile(&v.id),
        AuthzTopic::Comment(c, _op) => Topic::Comment(c.id),
        AuthzTopic::Other(_t, _op) => Topic::None,
    };
    if let Some(details) = details {
        send_user_error!(user_id.to_string(), server, topic, msg, details, true);
    } else {
        send_user_error!(user_id.to_string(), server, topic, msg);
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
    session: &proto::org::UserSessionData,
    desc: &str,
    msg_on_deny: bool,
    server: &ServerState,
    organizer: &Option<Arc<tokio::sync::Mutex<OrganizerConnection>>>,
    op: AuthzTopic<'a>,
) -> Option<bool>
{
    let user_id = match &session.user {
        Some(ui) => ui.id.clone(),
        None => {
            tracing::error!(op=?op, desc, "No user ID in session. Cannot check authz -- denying by default");
            return Some(false);
        }
    };

    let org = match &organizer {
        Some(org) => org,
        None => { return None; }
    };
    tracing::debug!(op=?op, user=user_id, desc, "Checking authz from Organizer");

    use proto::org::authz_user_action_request as authz_op;
    let pop = match op {
        AuthzTopic::MediaFile(v, op) => authz_op::Op::MediaFileOp(
            authz_op::MediaFileOp {
                op: op.into(),
                media_file: Some(v.to_proto3(&server.url_base, vec![])) }), // omit subtitles for authz check
        AuthzTopic::Comment(c, op) => authz_op::Op::CommentOp(
            authz_op::CommentOp {
                op: op.into(),
                comment: Some(c.to_proto3()) }),
        AuthzTopic::Other(subj, op) => authz_op::Op::OtherOp(
            authz_op::OtherOp {
                op: op.into(),
                subject: subj.map(|s| s.into()) }),
    };
    let req = proto::org::AuthzUserActionRequest { ses: Some(session.clone()), op: Some(pop) };
    let res = org.lock().await.authz_user_action(req).await;
    match res {
        Err(e) => {
            if e.code() == tonic::Code::Unimplemented {
                tracing::debug!(desc, user=user_id, "Organizer doesn't support authz");
                None
            } else if e.code() == tonic::Code::Aborted {
                tracing::warn!(desc, user=user_id, "Organizer gRPC.ABORTED authz request. Unsupported behavior for authz_user_action. Denying by default.");
                Some(false)
            } else {
                error!(desc, user=&user_id, err=?e, "Error while authorizing user action");
                try_send_error(&user_id, &server, format!("Internal error in authz: {}", desc), None, &op).ok();
                Some(false)
            }
        },
        Ok(res) => {
            match res.get_ref().is_authorized {
                Some(false) => {
                    let msg = res.get_ref().message.clone().map(|s| s).unwrap_or_else(|| "Permission denied".to_string());
                    let details = res.get_ref().details.clone();
                    if msg_on_deny { try_send_error(&user_id, &server, msg, details, &op).ok(); }
                    debug!(desc, user=user_id, "Organizer: Permission denied");
                    Some(false)
                },
                Some(true) => {
                    debug!(desc, user=user_id, "Organizer: Authorized OK");
                    Some(true)
                },
                None => {
                    debug!(desc, user=user_id, "Organizer: don't care, use defaults");
                    None
                }
            }
        }
    }
}

pub async fn org_authz_with_default<'a>(
    session: &proto::org::UserSessionData,
    desc: &str,
    msg_on_deny: bool,
    server: &ServerState,
    organizer: &Option<Arc<tokio::sync::Mutex<OrganizerConnection>>>,
    default: bool,
    op: AuthzTopic<'a>,
) -> Result<(), AuthzError> {
    if let Some(res) = org_authz(session, desc, msg_on_deny, server, organizer, op.clone()).await {
        if res { Ok(()) } else { Err(AuthzError::Denied) }
    } else {
        if default { Ok(()) } else {
            if msg_on_deny {
                if let Some(ui) = &session.user {
                    try_send_error(&ui.id, &server, format!("Permission denied: {}", desc), Some(format!("{:?}", &op)), &op).ok();
                } else {
                    tracing::error!(desc, "No user ID in session. Couldn't send deny message from org_authz_with_default");
                }
            };
            Err(AuthzError::Denied)
        }
    }
}
