//#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

use async_std::task::block_on;
use lib_clapshot_grpc::GrpcBindAddr;
use lib_clapshot_grpc::proto;
use lib_clapshot_grpc::proto::org::OnStartUserSessionResponse;
use tracing::debug;
use warp::Filter;
use core::panic;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use parking_lot::RwLock;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use warp::ws::Message;
use warp::http::HeaderMap;
use std::sync::atomic::Ordering::Relaxed;

use anyhow::{anyhow, bail};

pub mod server_state;
use server_state::ServerState;

pub mod user_session;

pub mod ws_handers;
use ws_handers::msg_dispatch;

#[macro_use]
#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
pub mod tests;
mod file_upload;
use file_upload::handle_multipart_upload;
use crate::api_server::user_session::AuthzTopic;
use crate::api_server::user_session::org_authz;
use crate::client_cmd;
use crate::database::DbBasicQuery;
use crate::database::models;
use crate::grpc::db_models::proto_msg_type_to_event_name;
use crate::grpc::grpc_client::OrganizerConnection;
use crate::grpc::grpc_client::OrganizerURI;
use crate::grpc::{grpc_server, make_media_file_popup_actions};
use crate::api_server::ws_handers::SessionClose;
use crate::video_pipeline::IncomingFile;
use self::user_session::UserSession;

type Res<T> = anyhow::Result<T>;
type WsMsgSender = tokio::sync::mpsc::UnboundedSender<Message>;
type SenderList = Vec<WsMsgSender>;
type SenderListMap = Arc<RwLock<HashMap<String, SenderList>>>;
type StringToStringMap = Arc<RwLock<HashMap<String, String>>>;
type SessionMap = Arc<RwLock<HashMap<String, UserSession>>>;

pub enum SendTo<'a> {
    UserSession(&'a str),
    UserId(&'a str),
    MediaFileId(&'a str),
    MsgSender(&'a WsMsgSender),
    Collab(&'a str),
}

pub type UserMessageTopic = proto::user_message::Type;

/// Message from other server modules to user(s)
#[derive (Clone, Debug, Default)]
pub struct UserMessage {
    pub topic: UserMessageTopic,
    pub user_id: Option<String>,
    pub msg: String,
    pub details: Option<String>,
    pub media_file_id: Option<String>,
    pub progress: Option<f32>,
}

fn abbrv(msg: &str) -> String {
    if msg.len() > 200 { msg[..200].to_string() + " (...)" } else { msg.to_string() }
}


/// User has connected to our WebSocket endpoint.
/// This function will run (potentially forever) for each individual user that connects.
async fn handle_ws_session(
        ws: warp::ws::WebSocket,
        sid: String,
        user_id: String,
        username: String,
        is_admin: bool,
        cookies: HashMap<String, String>,
        server: ServerState)
{
    let (msgq_tx, mut msgq_rx) = tokio::sync::mpsc::unbounded_channel();

    let user = match server.db.conn().and_then(|mut conn|
        models::User::get_or_create(&mut conn, &user_id, Some(&username)))
    {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(details=%e, "Error getting user info from DB. Closing session.");
            return;
        }
    };

    let mut ses = UserSession {
        sid: sid.clone(),
        sender: msgq_tx,
        user_id: user.id.clone(),
        user_name: user.name.clone(),
        is_admin,
        cur_media_file_id: None,
        cur_collab_id: None,
        media_session_guard: None,
        collab_session_guard: None,
        organizer: None,
        org_session: proto::org::UserSessionData {
            sid: sid.clone(),
            user: Some(proto::UserInfo {
                id: user_id.clone(),
                name: username.clone(),
            }),
            is_admin,
            cookies,
        }
    };

    let _user_session_guard = Some(server.register_user_session(&sid, &user_id, ses.clone()));
    let (mut ws_tx, mut ws_rx) = ws.split();

    // Let the client know user's id and name
    if let Err(e) = server.emit_cmd(
        client_cmd!(Welcome, {
            user: Some(proto::UserInfo { id: user_id, name: username }),
            is_admin: is_admin
        }),
        SendTo::MsgSender(&ses.sender)
    ) {
        tracing::error!(details=%e, "Error sending welcome message. Closing session.");
        return;
    }

    async fn connect_organizer(uri: OrganizerURI, ses: &proto::org::UserSessionData) -> Res<(OrganizerConnection, OnStartUserSessionResponse)> {
        let mut c = crate::grpc::grpc_client::connect(uri).await?;
        let start_ses_req = proto::org::OnStartUserSessionRequest { ses: Some(ses.clone()) };

        let res = match c.on_start_user_session(start_ses_req).await {
            Ok(res) => res.into_inner(),
            Err(e) => {
                if e.code() == tonic::Code::Unimplemented {
                    tracing::debug!("Organizer does not implement on_start_user_session. Ignoring.",);
                    OnStartUserSessionResponse::default()
                } else {
                    return Err(e.into())
                }
            }
        };
        Ok((c, res))
    }

    // Define default actions. Organizer may call DefineActions later to override these.
    if let Err(e) = server.emit_cmd(
            client_cmd!(DefineActions, {actions: make_media_file_popup_actions()}),
            SendTo::MsgSender(&ses.sender)) {
        tracing::error!(details=%e, "Error sending define_actions to client. Closing session.");
        return;
    }

    // Tell organizer about this new user session
    if let Some(uri) = server.organizer_uri.clone() {
        match connect_organizer(uri, &ses.org_session).await {
            Ok((c, _res)) => {
                ses.organizer = Some(tokio::sync::Mutex::new(c).into());
                let op = AuthzTopic::Other(None, proto::org::authz_user_action_request::other_op::Op::Login);
                if org_authz(&ses.org_session, "login", true, &server, &ses.organizer, op).await == Some(false) {
                    tracing::info!("User '{}' not authorized to login. Closing session.", ses.user_id);
                    server.emit_cmd(
                        client_cmd!(Error, {msg: "Login permission denied.".into()}),
                        SendTo::MsgSender(&ses.sender)).ok();
                    return;
                }
            },
            Err(e) => {
                const MSG: &str = "Error connecting to Organizer. Closing session.";
                tracing::error!(details=%e, MSG);
                server.emit_cmd(
                    client_cmd!(Error, {msg: MSG.into()}),
                    SendTo::MsgSender(&ses.sender)).ok();
                return;
            }
        }
    };

    loop
    {
        tokio::select!
        {
            // Termination flag set? Exit.
            _ = sleep(Duration::from_millis(100)) => {
                if server.terminate_flag.load(Relaxed) {
                    tracing::debug!("Termination flag set. Closing session.");
                    break;
             }},

            // Message in queue? Send to client.
            Some(msg) = msgq_rx.recv() => {
                tracing::debug!(msg = abbrv(msg.to_str().unwrap_or("<msg.to_str() failed>")), "Sending message to client.");
                if let Err(e) = ws_tx.send(msg).await {
                    tracing::error!(details=%e, "Error sending message - closing session.");
                    break;
                }
            },

            // Message from client? Handle it.
            Some(msg) = ws_rx.next() => {
                match msg {
                    Err(e) => {
                        tracing::debug!(details=%e, "Error receiving message - closing session.");
                        break;
                    },
                    Ok(msg) => {
                        if msg.is_text() {

                            fn parse_msg(msg: &Message) -> Res<(String, serde_json::Value, HashMap<String, String>)> {
                                let msg_str = msg.to_str().unwrap_or("!!msg was supposed to .is_text()!!");

                                let mut json: serde_json::Value = serde_json::from_str(msg_str)?;
                                let mut cookies = HashMap::new();

                                if let serde_json::Value::Object(map) = &mut json {
                                    if let Some(cookies_json) = map.get("cookies") {
                                        if let Some(cookies_json) = cookies_json.as_object() {
                                            for (k, v) in cookies_json {
                                                if let Some(v) = v.as_str() {
                                                    cookies.insert(k.clone(), v.to_string());
                                                }}}
                                        map.remove("cookies");
                                        assert!(json.get("cookies").is_none());
                                    }
                                } else {
                                    bail!("JSON message was not a dict.");
                                }
                                let cmd_str = json.as_object().unwrap().keys().next().ok_or(anyhow!("JSON message had no command."))?.clone();
                                Ok((cmd_str, json, cookies))
                            }

                            tracing::debug!("Msg from client. Raw text: {}", abbrv(msg.to_str().unwrap_or("<msg.to_str() failed>")));

                            let (cmd_str, json) = match parse_msg(&msg) {
                                Ok((cmd, json, cookies)) => {
                                    ses.org_session.cookies = cookies;
                                    (cmd, json)
                                },
                                Err(e) => {
                                    tracing::warn!(details=%e, "Error parsing JSON message. Closing session.");
                                    #[cfg(not(test))] {
                                        sleep(Duration::from_secs(5)).await;
                                    }
                                    let err_msg = proto::client::server_to_client_cmd::Error { msg: format!("Invalid message, bye -- {}", e) };
                                    let json_txt = serde_json::to_string(&err_msg).expect("Error serializing error message");
                                    ws_tx.send(Message::text(json_txt)).await.ok();
                                    break;
                                }
                            };
                            tracing::debug!(cmd=%cmd_str, "Msg from client");

                            match serde_json::from_value::<proto::client::ClientToServerCmd>(json.clone()) {
                                Ok(req) => {
                                    match msg_dispatch(&req, &mut ses, &server, ).await {
                                        Ok(true) => {},             // Continues serving
                                        Ok(false) => { break; }     // Session closed
                                        Err(e) => {
                                            if let Some(e) = e.downcast_ref::<SessionClose>() {
                                                if !matches!(e, SessionClose::Logout) { tracing::debug!("[{}] Closing session: {:?}", sid, e); }
                                                break;
                                            } else if let Some(e) = e.downcast_ref::<tokio::sync::mpsc::error::SendError<Message>>() {
                                                tracing::error!("[{}] Error sending message. Closing session. -- {}", sid, e);
                                                break;
                                            } else {
                                                let answ = format!("Error handling command '{}'.", cmd_str);
                                                tracing::warn!("[{}] {}: {}", sid, answ, e);
                                                let err_msg = proto::client::server_to_client_cmd::Error { msg: answ };
                                                let json_txt = serde_json::to_string(&err_msg).expect("Error serializing error message");
                                                if ws_tx.send(Message::text(json_txt)).await.is_err() { break; };
                                            }
                                        }
                                    };
                                },
                                Err(e) => {
                                    tracing::warn!(details=%e, "Invalid command from client: {:?}", cmd_str);
                                    let err_msg = proto::client::server_to_client_cmd::Error { msg: format!("Invalid command from client: {:?}", e) };
                                    let json_txt = serde_json::to_string(&err_msg).expect("Error serializing error message");
                                    ws_tx.send(Message::text(json_txt)).await.ok();
                                }
                            };
                        } else if msg.is_close() {
                            tracing::debug!("Got websocket close message.");
                            break
                        } else {
                            tracing::error!(msg=?msg, "Got unexpected message - closing session.");
                            break
                        };
                    }
                }
            },

            else => {
                if ses.sender.is_closed() {
                    tracing::info!("Sender channel gone. Closing session.");
                    break;
                }
            }
        }
    }
}

/// Extract user id, name and clapshot_cookies from HTTP headers (set by nginx)
/// If any of the headers are missing, default values are used:
/// - If X-Remote-User-Id is missing, `default_user_id` is used.
/// - If X-Remote-User-Name is missing, the user ID is used as the name.
/// - If X-Remote-User-Is-Admin is missing, user is admin iff user_id == "admin",
///   otherwise, if the header is present, it must be "true" or "1" to be an admin.
///
/// # Arguments
/// * `hdrs` - HTTP headers
/// * `default_user_id` - Default user ID to use if X-Remote-User-Id is missing
///
/// * Returns: (user_id: String, user_name: String, is_admin: bool, clapshot_cookies: HashMap<String, String>)
fn parse_auth_headers(hdrs: &HeaderMap, default_user_id: &str) -> (String, String, bool, HashMap<String, String>)
{
    fn try_get_first_named_hdr<T>(hdrs: &HeaderMap, names: T) -> Option<String>
        where T: IntoIterator<Item=&'static str> {
        for n in names {
            if let Some(val) = hdrs.get(n).or(hdrs.get(n.to_lowercase())) {
                match val.to_str() {
                    Ok(s) => return Some(s.into()),
                    Err(e) => tracing::warn!(details=%e, "Error parsing header '{}'.", n),
        }}}
        None
    }

    let user_id = match try_get_first_named_hdr(&hdrs, vec!["X-Remote-User-Id", "X_Remote_User_Id", "HTTP_X_REMOTE_USER_ID"]) {
        Some(id) => id,
        None => {
            tracing::warn!("Missing X-Remote-User-Id in HTTP headers. Using '{}' instead.", default_user_id);
            default_user_id.into()
        }};
    let user_name = try_get_first_named_hdr(&hdrs, vec!["X-Remote-User-Name", "X_Remote_User_Name", "HTTP_X_REMOTE_USER_NAME"])
        .unwrap_or_else(|| user_id.clone());

    let cookies_str = try_get_first_named_hdr(&hdrs, vec!["X-Clapshot-Cookies", "X_Clapshot_Cookies", "HTTP_X_CLAPSHOT_COOKIES"])
        .unwrap_or_else(|| "{}".into());

    let is_admin: bool = try_get_first_named_hdr(&hdrs, vec!["X-Remote-User-Is-Admin", "X_Remote_User_Is_Admin", "HTTP_X_REMOTE_USER_IS_ADMIN"])
        .map(|s| s.to_lowercase() == "true" || s == "1").unwrap_or(user_id == "admin");

    let app_cookies = match cookies_str.parse::<serde_json::Value>() {
        Ok(c) => {
            match c.as_object() {
                Some(c) => c.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("<!ERROR: NON-STRING COOKIE VALUE!>").to_string())).collect(),
                None => {
                    tracing::error!("'clapshot_cookies' was not a JSON dict, ignoring.");
                    HashMap::new()
                }
            }
        },
        Err(e) => {
            tracing::error!("Error parsing 'clapshot_cookies' JSON: {}", e);
            HashMap::new()
        }
    };

    (user_id, user_name, is_admin, app_cookies)
}

/// Handle HTTP requests, read authentication headers and dispatch to WebSocket handler.
async fn run_api_server_async(
    bind_addr: std::net::IpAddr,
    cors_origins: Vec<String>,
    server_state: ServerState,
    user_msg_rx: crossbeam_channel::Receiver<UserMessage>,
    upload_results_tx: crossbeam_channel::Sender<IncomingFile>,
    grpc_server_bind: Option<GrpcBindAddr>,
    port: u16)
{
    let session_counter = Arc::new(RwLock::new(0u64));
    let server_state_cln1 = server_state.clone();
    let server_state_cln2 = server_state.clone();
    let server_state_cln3 = server_state.clone();

    let url_base = server_state.url_base.clone();

    // Start gRPC server.
    // At this point, we have already connected to the Organizer in the
    // other direction, so we know that the Organizer is up and running,
    // and are waiting for it to connect to back to us for bidirectional gRPC.
    let grpc_server = match grpc_server_bind {
        Some(bind) => {
            tracing::info!("Starting gRPC server for org->srv.");
            let server = server_state.clone();
            let b = bind.clone();
            let hdl = tokio::spawn(async move {
                grpc_server::run_org_to_srv_grpc_server(b, server).await
            });
            let server = server_state.clone();
            let mut wait_time = Duration::from_millis(10);
            sleep(wait_time).await;
            while !server.organizer_has_connected.load(Relaxed) {
                sleep(wait_time).await;
                if wait_time > Duration::from_secs(1) {
                    tracing::debug!("Waiting for org->srv connection...");
                }
                wait_time = std::cmp::min(wait_time * 2, Duration::from_secs(4));
                if server.terminate_flag.load(Relaxed) { return; }
            }
            if let Some(org_info) = server.organizer_info.lock().await.as_ref() {
                tracing::info!(
                    org_name = &org_info.name,
                    description = &org_info.description,
                    version = org_info.version.as_ref().map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch)),
                    "org->srv connected, bidirectional gRPC established.");
            } else {
                panic!("Organizer connected, but no info received. This is a bug in server code.");
            }
            Some(hdl)
        },
        None => None,
    };

    tracing::info!(port=port, "Starting websocket API.");

    let rt_health = warp::path("api").and(warp::path("health")).map(|| "I'm alive!");

    let upload_dir = server_state.upload_dir.clone();
    let rt_upload = warp::path("api").and(warp::path("upload"))
        .and(warp::post())
        .and(warp::any().map(move || upload_dir.clone()))
        .and(warp::any().map(move || upload_results_tx.clone()))
        .and(warp::header::<mime::Mime>("content-type"))
        .and(warp::header::headers_cloned())
        .and(warp::any().map(move || server_state_cln3.clone()))
        .and(warp::body::stream())
        .and_then(handle_multipart_upload);

    let rt_videos = warp::path("videos").and(
        warp::fs::dir(server_state_cln1.media_files_dir.clone())
            .with(warp::log("videos")));

    let rt_api_ws = warp::path("api").and(warp::path("ws"))
        .and(warp::header::headers_cloned())
        .and(warp::ws())
        .map (move|hdrs: HeaderMap, ws: warp::ws::Ws| {

            // Get user ID and username (from reverse proxy)
            let (user_id, user_name, is_admin, app_cookies) = parse_auth_headers(&hdrs, &server_state.default_user);

            // Increment session counter
            let sid = {
                let mut counter = session_counter.write();
                *counter += 1;
                (*counter).to_string()
            };

            let server_state = server_state.clone();
            let is_admin = is_admin.clone();
            ws.on_upgrade(move |ws| async move {
                // Diesel SQLite calls are blocking, so run a thread per user session
                // even though we're using async/await
                tokio::task::spawn_blocking(move || {
                    let _span = tracing::info_span!("ws_session", sid=%sid, user=%user_id).entered();
                    block_on(handle_ws_session(ws, sid, user_id, user_name, is_admin, app_cookies, server_state));
                }).await.unwrap_or_else(|e| {
                    tracing::error!(details=%e, "Error joining handle_ws_session thread."); });
            })
        });

    let routes = rt_health.or(rt_api_ws).or(rt_upload).or(rt_videos)
        .with(warp::log("api_server"));


    let mut cors_origins: Vec<&str> = cors_origins.iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    tracing::info!("Allowed CORS origins: {:?}", cors_origins);

    let cors_methods = ["GET", "POST", "HEAD", "OPTIONS"];
    let cors_headers = ["x-file-name", "x-clapshot-cookies", "content-type", "upgrade", "sec-websocket-protocol", "sec-websocket-version"];

    let routes = if cors_origins.contains(&"*") {
        tracing::warn!(concat!(
            "!! SECURITY RISK !! – Using CORS origin '*' allows any website to access your system. ",
            "This exposes your users' files to potential API attacks. ",
            "Do NOT use '*' in production! ",
            "Instead, specify the allowed origin, such as 'https://clapshot.example.com'."
        ));
        routes.with(warp::cors().allow_methods(cors_methods).allow_headers(cors_headers)
            .allow_any_origin()).boxed()
    } else {
        if cors_origins.is_empty() {
            cors_origins.push(url_base.as_str());
            tracing::info!("No CORS origins specified. Using url_base for it: '{}'", url_base);
        } else {
            tracing::info!("Using CORS origins: {:?}", cors_origins);
        }
        routes.with(warp::cors().allow_methods(cors_methods).allow_headers(cors_headers)
            .allow_origins(cors_origins)).boxed()
    };

    debug!("Binding Websocket API to {}:{}", bind_addr, port);
    let (_addr, server) = warp::serve(routes)
        .bind_with_graceful_shutdown((bind_addr, port), async move {
            while !server_state_cln1.terminate_flag.load(Relaxed) {
                sleep(Duration::from_millis(100)).await;
            }
        });

    let server_state = server_state_cln2;
    let msg_relay = async move {
        while !server_state.terminate_flag.load(Relaxed) {
            sleep(Duration::from_millis(100)).await;
            while let Ok(m) = user_msg_rx.try_recv() {
                let topic_str = proto_msg_type_to_event_name(m.topic);

                let msg_insert = models::MessageInsert  {
                    event_name: topic_str.into(),
                    user_id: m.user_id.clone().unwrap_or("".into()),
                    message: m.msg.clone(),
                    details: m.details.clone().unwrap_or("".into()),
                    seen: false, comment_id: None,
                    media_file_id: m.media_file_id.clone()
                };

                let mut proto_msg = msg_insert.to_proto3();
                proto_msg.progress = m.progress;

                // Message to all watchers of a media file
                if let Some(vid) = m.media_file_id {
                    if let Err(_) = server_state.emit_cmd(
                        client_cmd!(ShowMessages, { msgs: vec![proto_msg.clone()] }),
                        SendTo::MediaFileId(&vid)
                    ) {
                        tracing::error!(media_file=vid, "Failed to send notification to media file watchers.");
                    }
                };

                // Message to a single user
                // Save it to the database, marking it as seen if sending it to the user succeeds
                if let Some(user_id) = m.user_id {
                    let mut user_was_online = false;
                    match server_state.emit_cmd(
                        client_cmd!(ShowMessages, { msgs: vec![proto_msg.clone()] }),
                        SendTo::UserId(&user_id))
                    {
                        Ok(session_cnt) => { user_was_online = session_cnt>0 },
                        Err(e) => tracing::error!(user=user_id, details=%e, "Failed to send user notification."),
                    }
                    if !(matches!(m.topic, UserMessageTopic::Progress | UserMessageTopic::MediaFileAdded | UserMessageTopic::MediaFileUpdated)) {
                        let msg = models::MessageInsert {
                            seen: msg_insert.seen || user_was_online,
                            ..msg_insert
                        };
                        server_state.db.conn()
                            .and_then(|mut conn| models::Message::insert(&mut conn, &msg))
                            .map_err(|e| tracing::error!(details=%e, "Failed to save user notification in DB."))
                            .ok();
                    }
                };
            }
        };
        server_state.terminate_flag.store(true, Relaxed);
    };

    // Start API server + message relay and wait for them to exit
    tokio::join!(server, msg_relay);

    // Wait for gRPC server to exit
    if let Some(g) = grpc_server {
        debug!("Waiting for gRPC server to exit...");
        match tokio::try_join!(g) {
            Ok((Ok(_),)) => tracing::debug!("gRPC server for org->srv exited OK."),
            Ok((Err(e),)) => tracing::error!(details=%e, "gRPC server for org->srv exited with error."),
            Err(e) => tracing::error!(details=%e, "gRPC server for org->srv panicked."),
        };
    }

    tracing::debug!("Exiting.");
}


#[tokio::main]
pub async fn run_forever(
    user_msg_rx: crossbeam_channel::Receiver<UserMessage>,
    grpc_server_bind: Option<GrpcBindAddr>,
    upload_res_tx: crossbeam_channel::Sender<IncomingFile>,
    bind_addr: String,
    url_base: String,
    cors_origins: Vec<String>,
    state: ServerState,
    port: u16)
{
    assert!(!url_base.ends_with('/')); // Should have been stripped by caller

    let bind_addr = match bind_addr.parse::<std::net::IpAddr>() {
        Ok(ip) => ip,
        Err(_) => {
            tracing::error!("Failed to parse bind address: '{}'", bind_addr);
            return;
        }
    };

    let _span = tracing::info_span!("API").entered();
    run_api_server_async(bind_addr, cors_origins, state, user_msg_rx, upload_res_tx, grpc_server_bind, port).await;
}
