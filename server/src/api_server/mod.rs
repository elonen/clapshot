//#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

use async_std::task::block_on;
use tracing::debug;
use warp::Filter;
use std::collections::HashMap;
use std::sync::{Arc};
use std::time::Duration;
use tokio::time::sleep;
use parking_lot::RwLock;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use warp::ws::{Message};
use warp::http::HeaderMap;
use std::sync::atomic::Ordering::Relaxed;

use anyhow::{anyhow, bail};

pub mod server_state;
use server_state::ServerState;

pub mod user_session;

mod ws_handers;
use ws_handers::msg_dispatch;

#[macro_use]
#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
pub mod tests;
mod file_upload;
use file_upload::handle_multipart_upload;
use crate::database::{models};
use crate::grpc::grpc_server;
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
    VideoHash(&'a str),
    MsgSender(&'a WsMsgSender),
    Collab(&'a str),
}

#[derive (Clone, Debug)]
pub enum UserMessageTopic { Ok(), Error(), Progress(), VideoUpdated() }

/// Message from other server modules to user(s)
#[derive (Clone, Debug)]
pub struct UserMessage {
    pub topic: UserMessageTopic,
    pub user_id: Option<String>,
    pub msg: String,
    pub details: Option<String>,
    pub video_hash: Option<String>
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
        server: ServerState)
{
    let (msgq_tx, mut msgq_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut ses = UserSession {
        sid: sid.clone(),
        sender: msgq_tx,
        user_id: user_id.clone(),
        user_name: username.clone(),
        cur_video_hash: None,
        cur_collab_id: None,
        video_session_guard: None,
        collab_session_guard: None,
    };

    let _user_session_guard = Some(server.register_user_session(&sid, &user_id, ses.clone()));
    let (mut ws_tx, mut ws_rx) = ws.split();

    // Let the client know user's id and name
    if let Err(e) = server.emit_cmd("welcome", 
            &serde_json::json!({ "user_id": user_id, "username": username }), 
            SendTo::MsgSender(&ses.sender)) {
        tracing::error!(details=%e, "Error sending welcome message. Closing session.");
        return;
    }

    loop
    {
        tokio::select!
        {
            // Termination flag set? Exit.
            _ = sleep(Duration::from_millis(100)) => {
                if server.terminate_flag.load(Relaxed) {
                    tracing::info!("Termination flag set. Closing session.");
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

                            fn parse_msg(msg: &Message) -> Res<(String, serde_json::Value)> {
                                let msg_str = msg.to_str().unwrap_or("!!msg was supposed to .is_text()!!");
                                let json: serde_json::Value = serde_json::from_str(msg_str)?;
                                let cmd = json["cmd"].as_str().ok_or(anyhow!("Missing cmd"))?.trim().to_string();

                                if cmd.len() == 0 || cmd.len() > 64 { bail!("Bad cmd") }
                                let data = json.get("data").unwrap_or(&serde_json::json!({})).clone();

                                // Check data fields for length. Only "drawing" is allowed to be long.
                                for (k, v) in data.as_object().unwrap_or(&serde_json::Map::new()) {
                                    if k != "drawing" && v.as_str().map(|s| s.len() > 2048).unwrap_or(false) { bail!("Field too long"); }
                                }                                
                                Ok((cmd, data))
                            }

                            let (cmd, data) = match parse_msg(&msg) {
                                Ok((cmd, data)) => (cmd, data),
                                Err(e) => {
                                    tracing::warn!(details=%e, "Error parsing JSON message. Closing session.");
                                    #[cfg(not(test))] {
                                        sleep(Duration::from_secs(5)).await;
                                    }
                                    let answ = format!("Invalid message, bye -- {}", e);
                                    ws_tx.send(Message::text(format!(r#"{{"cmd":"error", "data":{{"message": "{}"}}}}"#, answ))).await.ok();
                                    break;
                                }
                            };
                            tracing::debug!(cmd=%cmd, "Msg from client.");
                            match msg_dispatch(&cmd, &data, &mut ses, &server).await {
                                Ok(true) => {},
                                Ok(false) => { break; }
                                Err(e) => {
                                    if let Some(e) = e.downcast_ref::<tokio::sync::mpsc::error::SendError<Message>>() {
                                        tracing::error!("[{}] Error sending message. Closing session. -- {}", sid, e);
                                        break;
                                    } else {
                                        let answ = format!("Error handling command '{}'.", cmd);
                                        tracing::warn!("[{}] {}: {}", sid, answ, e);
                                        if ws_tx.send(Message::text(format!(r#"{{"cmd":"error", "data":{{"message": "{}"}}}}"#, answ))).await.is_err() { break; };
                                    }
                                }

                            };
                        } else if msg.is_close() {
                            tracing::info!("Got websocket close message.");
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

/// Extract user id and name from HTTP headers (set by nginx)
fn parse_auth_headers(hdrs: &HeaderMap) -> (String, String) 
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
            tracing::warn!("Missing X-Remote-User-Id in HTTP headers. Using 'anonymous' instead.");
            "anonymous".into()
        }};
    let user_name = try_get_first_named_hdr(&hdrs, vec!["X-Remote-User-Name", "X_Remote_User_Name", "HTTP_X_REMOTE_USER_NAME"])
        .unwrap_or_else(|| user_id.clone());
    
    (user_id, user_name)
}

/// Handle HTTP requests, read authentication headers and dispatch to WebSocket handler.
async fn run_api_server_async(
    bind_addr: std::net::IpAddr,
    server_state: ServerState,
    user_msg_rx: crossbeam_channel::Receiver<UserMessage>,
    upload_results_tx: crossbeam_channel::Sender<IncomingFile>,
    grpc_server_bind: Option<grpc_server::BindAddr>,
    port: u16)
{
    let session_counter = Arc::new(RwLock::new(0u64));
    let server_state_cln1 = server_state.clone();
    let server_state_cln2 = server_state.clone();

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
                grpc_server::run_grpc_server(b, server).await
            });
            let server = server_state.clone();
            let mut wait_time = Duration::from_millis(10);
            sleep(wait_time).await;
            while !server.organizer_has_connected.load(Relaxed) {
                sleep(wait_time).await;
                if wait_time > Duration::from_secs(1) {
                    tracing::info!("Waiting for org->srv connection...");
                }
                wait_time = std::cmp::min(wait_time * 2, Duration::from_secs(4));
                if server.terminate_flag.load(Relaxed) { return; }
            }
            tracing::debug!("org->srv connected");
            Some(hdl)
        },
        None => None,
    };

    tracing::info!(port=port, "Starting frontend API server.");

    let rt_health = warp::path("api").and(warp::path("health")).map(|| "I'm alive!");

    let upload_dir = server_state.upload_dir.clone();
    let rt_upload = warp::path("api").and(warp::path("upload"))
        .and(warp::post())
        .and(warp::any().map(move || upload_dir.clone()))
        .and(warp::any().map(move || upload_results_tx.clone()))
        .and(warp::header::<mime::Mime>("content-type"))
        .and(warp::header::headers_cloned())
        .and(warp::body::stream())
        .and_then(handle_multipart_upload);

    let rt_videos = warp::path("videos").and(
        warp::fs::dir(server_state_cln1.videos_dir.clone())
            .with(warp::log("videos")));

    let rt_api_ws = warp::path("api").and(warp::path("ws"))
        .and(warp::header::headers_cloned())
        .and(warp::ws())
        .map (move|hdrs: HeaderMap, ws: warp::ws::Ws| {

            // Get user ID and username (from reverse proxy)
            let (user_id, user_name) = parse_auth_headers(&hdrs);

            // Increment session counter
            let sid = {
                let mut counter = session_counter.write();
                *counter += 1;
                (*counter).to_string()
            };

            let server_state = server_state.clone();
            ws.on_upgrade(|ws| async {
                // Diesel SQLite calls are blocking, so run a thread per user session
                // even though we're using async/await
                tokio::task::spawn_blocking( move || {
                    let _span = tracing::info_span!("ws_session", sid=%sid, user=%user_id).entered();
                    block_on(handle_ws_session(ws, sid, user_id, user_name, server_state));
                }).await.unwrap_or_else(|e| {
                    tracing::error!(details=%e, "Error joining handle_ws_session thread."); });
            })
        });

    let routes = rt_health.or(rt_api_ws).or(rt_upload).or(rt_videos);

    let routes = routes.with(warp::log("api_server"))
        .with(warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST"])
        .allow_headers(vec!["x-file-name"]));


    debug!("Binding to {}:{}", bind_addr, port);
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
                let topic_str = match m.topic{
                    UserMessageTopic::Ok() => "ok",
                    UserMessageTopic::Error() => "error",
                    UserMessageTopic::Progress() => "progress",
                    UserMessageTopic::VideoUpdated() => "video_updated",
                };

                let msg = models::MessageInsert  {
                    event_name: topic_str.into(),
                    user_id: m.user_id.clone().unwrap_or("".into()),
                    message: m.msg.clone(),
                    details: m.details.clone().unwrap_or("".into()),
                    seen: false, ref_comment_id: None,
                    ref_video_hash: m.video_hash.clone()
                };

                // Message to all watchers of a video
                if let Some(vh) = m.video_hash {
                    if let Ok(data) = &msg.to_json() {
                        let msg = Message::text(serde_json::json!({
                            "cmd": "message", "data": data }).to_string());
                        if let Err(_) = server_state.send_to_all_video_sessions(&vh, &msg) {
                            tracing::error!(video=vh, "Failed to send notification to video hash.");
                        }
                    }        
                };

                // Message to a single user
                // Save it to the database, marking it as seen if sending it to the user succeeds
                if let Some(user_id) = m.user_id {
                    let mut user_was_online = false;
                    if let Ok(data) = msg.to_json() {
                        let msg = Message::text(serde_json::json!({
                            "cmd": "message", "data": data }).to_string());
                        match server_state.send_to_all_user_sessions(&user_id, &msg) {
                            Ok(session_cnt) => { user_was_online = session_cnt>0 },
                            Err(e) => tracing::error!(user=user_id, details=%e, "Failed to send user notification."),
                        }
                    }
                    if !matches!(m.topic, UserMessageTopic::Progress()) {
                        let msg = models::MessageInsert {
                            seen: msg.seen || user_was_online,
                            ..msg
                        };
                        if let Err(e) = server_state.db.add_message(&msg) {
                            tracing::error!(details=%e, "Failed to save user notification in DB.");
                        }
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
    grpc_server_bind: Option<grpc_server::BindAddr>,
    upload_res_tx: crossbeam_channel::Sender<IncomingFile>,
    bind_addr: String,
    url_base: String,
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
    run_api_server_async(bind_addr, state, user_msg_rx, upload_res_tx, grpc_server_bind, port).await;
}
