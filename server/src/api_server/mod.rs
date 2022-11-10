//#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

use async_std::task::block_on;
use warp::Filter;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool};
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use warp::ws::{Message};
use warp::http::HeaderMap;
use std::sync::atomic::Ordering::Relaxed;

use std::path::{PathBuf};
use anyhow::{anyhow, bail};

mod server_state;
use server_state::ServerState;

mod ws_handers;
use ws_handers::msg_dispatch;

#[macro_use]
#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
pub mod tests;

mod file_upload;
use file_upload::handle_multipart_upload;

use crate::database::{models, DB};
use crate::video_pipeline::IncomingFile;

type Res<T> = anyhow::Result<T>;
type WsMsgSender = tokio::sync::mpsc::UnboundedSender<Message>;
type SenderList = Vec<WsMsgSender>;
type SenderListMap = Arc<RwLock<HashMap<String, SenderList>>>;


pub enum SendTo<'a> {
    CurSession(),
    UserId(&'a str),
    VideoHash(&'a str),
    MsgSender(&'a WsMsgSender),
}

#[derive (Clone, Debug)]
pub enum UserMessageTopic { Ok(), Error(), Progress() }

/// Message from other server modules to user(s)
#[derive (Clone, Debug)]
pub struct UserMessage {
    pub topic: UserMessageTopic,
    pub user_id: Option<String>,
    pub msg: String,
    pub details: Option<String>,
    pub video_hash: Option<String>
}

pub struct WsSessionArgs<'a> {
    sid: &'a str,
    sender: &'a WsMsgSender,
    server: ServerState,
    user_id: &'a str,
    user_name: &'a str,
    video_session_guard: Option<Box<tokio::sync::Mutex<dyn Send>>>,
}

impl WsSessionArgs<'_> {

    /// Send a command to client websocket(s).
    /// 
    /// If send_to is a string, it is interpreted either as a video hash or user id.
    /// - If it turns out to be a video hash, the message is sent to all websocket
    ///     that are watching it.
    /// - If it's a user id, the message is sent to all websocket connections that user has open.
    /// - If it's a MsgSender, the message is sent to that connection only.
    /// - If it's a SendTo::CurSession, the message is sent to the current session only.
    pub fn emit_cmd(&self, cmd: &str, data: &serde_json::Value, send_to: SendTo) -> Res<u32>
    {
        let msg = serde_json::json!({ "cmd": cmd, "data": data });
        let msg = Message::text(msg.to_string());
        match send_to {
            SendTo::CurSession() => { self.sender.send(msg)?; Ok(1u32) },
            SendTo::UserId(user_id) => { self.server.send_to_all_user_sessions(user_id, &msg) },
            SendTo::VideoHash(video_hash) => { self.server.send_to_all_video_sessions(video_hash, &msg) },
            SendTo::MsgSender(sender) => { sender.send(msg)?; Ok(1u32) },
        }
    }
    
    pub fn push_notify_message(&self, msg: &models::MessageInsert, persist: bool) -> Res<()> {
        if persist {
            if let Err(e) = self.server.db.add_message(msg) {
                tracing::error!("Failed to persist user notification: {}", e);
                return Err(e.into());
            }};
        self.emit_cmd("message", &msg.to_json()?, SendTo::UserId(&msg.user_id)).map(|_| ())           
    }

    pub async fn emit_new_comment(&self, mut c: models::Comment, send_to: SendTo<'_>) -> Res<()> {
        if let Some(drawing) = &mut c.drawing {
            // If drawing is present, read it from disk and encode it into a data URI.
            if !drawing.starts_with("data:") {
                let path = self.server.videos_dir.join(&c.video_hash).join("drawings").join(&drawing);
                if path.exists() {
                    let data = tokio::fs::read(path).await?;
                    *drawing = format!("data:image/webp;base64,{}", base64::encode(&data));
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
        let mut fields = c.to_json()?;
        fields["comment_id"] = fields["id"].take();  // swap id with comment_id, because the client expects comment_id        
        self.emit_cmd("new_comment", &fields , send_to).map(|_| ())
    }

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
        server_state: ServerState)
{
    let (msgq_tx, mut msgq_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut ses = WsSessionArgs {
        sid: &sid,
        sender: &msgq_tx,
        server: server_state,
        user_id: &user_id,
        user_name: &username,
        video_session_guard: None,
    };

    let _user_session_guard = ses.server.register_user_session(&user_id, msgq_tx.clone());
    let (mut ws_tx, mut ws_rx) = ws.split();

    // Let the client know user's id and name
    if let Err(e) = ses.emit_cmd("welcome", 
            &serde_json::json!({ "user_id": user_id, "username": username }), 
            SendTo::CurSession()) {
        tracing::error!(details=%e, "Error sending welcome message. Closing session.");
        return;
    }

    loop
    {
        tokio::select!
        {
            // Termination flag set? Exit.
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                if ses.server.terminate_flag.load(Relaxed) {
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
                        tracing::error!(details=%e, "Error receiving message - closing session.");
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
                                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                    }
                                    let answ = format!("Invalid message, bye -- {}", e);
                                    ws_tx.send(Message::text(format!(r#"{{"cmd":"error", "data":{{"message": "{}"}}}}"#, answ))).await.ok();
                                    break;
                                }
                            };
                            tracing::debug!(cmd=%cmd, "Msg from client.");

                            if let Err(e) = msg_dispatch(&cmd, &data, &mut ses).await {
                                    if let Some(e) = e.downcast_ref::<tokio::sync::mpsc::error::SendError<Message>>() {
                                        tracing::error!("[{}] Error sending message. Closing session. -- {}", sid, e);
                                        break;
                                    } else {
                                        let answ = format!("Error handling command '{}'.", cmd);
                                        tracing::warn!("[{}] {}: {}", sid, answ, e);
                                        if ws_tx.send(Message::text(format!(r#"{{"cmd":"error", "data":{{"message": "{}"}}}}"#, answ))).await.is_err() { break; };
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
    server_state: ServerState,
    user_msg_rx: crossbeam_channel::Receiver<UserMessage>,
    upload_results_tx: crossbeam_channel::Sender<IncomingFile>,
    port: u16)
{
    let session_counter = Arc::new(RwLock::new(0u64));
    let server_state_cln1 = server_state.clone();
    let server_state_cln2 = server_state.clone();

    tracing::info!(port=port, "Starting API server.");

    let rt_hello = warp::path("hello").map(|| "Hello, World!");

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
                let mut counter = session_counter.write().unwrap();
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

    let routes = rt_hello.or(rt_api_ws).or(rt_upload).or(rt_videos);

    let routes = routes.with(warp::log("api_server"))
        .with(warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST"])
        .allow_headers(vec!["x-file-name"]));

    let (_addr, server) = warp::serve(routes)
        .bind_with_graceful_shutdown(([127, 0, 0, 1], port), async move {
            while !server_state_cln1.terminate_flag.load(Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });

    let server_state = server_state_cln2;
    let msg_relay = async move {
        while !server_state.terminate_flag.load(Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            while let Ok(m) = user_msg_rx.try_recv() {
                let topic_str = match m.topic{
                    UserMessageTopic::Ok() => "ok",
                    UserMessageTopic::Error() => "error",
                    UserMessageTopic::Progress() => "progress",
                };

                let msg = models::MessageInsert  {
                    event_name: topic_str.into(),
                    user_id: m.user_id.clone().unwrap_or("".into()),
                    message: m.msg.clone(),
                    details: m.details.clone().unwrap_or("".into()),
                    seen: false, ref_comment_id: None,
                    ref_video_hash: m.video_hash.clone()
                };
                
                // Message to a single user
                if let Some(user_id) = m.user_id {
                    if !matches!(m.topic, UserMessageTopic::Progress()) {
                        if let Err(e) = server_state.db.add_message(&msg) {
                            tracing::error!(details=%e, "Failed to save user notification in DB.");
                        }
                    }
                    if let Ok(data) = msg.to_json() {
                        let msg = Message::text(serde_json::json!({
                            "cmd": "message", "data": data }).to_string());
                        if let Err(e) = server_state.send_to_all_user_sessions(&user_id, &msg) {
                            tracing::error!(user=user_id, details=%e, "Failed to send user notification.");
                        }
                    }
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

            }
        };
    };

    tokio::join!(server, msg_relay);
    tracing::info!("Exiting.");
}


#[tokio::main]
pub async fn run_forever(
    db: Arc<DB>,
    videos_dir: PathBuf,
    upload_dir: PathBuf,
    user_msg_rx: crossbeam_channel::Receiver<UserMessage>,
    upload_res_tx: crossbeam_channel::Sender<IncomingFile>,
    terminate_flag: Arc<AtomicBool>,
    url_base: String,
    port: u16)
{
    assert!(!url_base.ends_with('/')); // Should have been stripped by caller
    let _span = tracing::info_span!("API").entered();
    let state = ServerState::new( db,
        &videos_dir,
        &upload_dir,
        &url_base,
        terminate_flag );
    run_api_server_async(state, user_msg_rx, upload_res_tx, port).await
}
