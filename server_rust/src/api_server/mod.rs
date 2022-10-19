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

use std::path::{Path, PathBuf};

mod server_state;
use server_state::ServerState;

mod ws_handers;
use ws_handers::msg_dispatch;

mod file_upload;
use file_upload::handle_multipart_upload;

use crate::database::DB;
use crate::database::models;


type Res<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type WsMsgSender = tokio::sync::mpsc::UnboundedSender<Message>;
type SenderList = Vec<WsMsgSender>;
type SenderListMap = Arc<RwLock<HashMap<String, SenderList>>>;


pub enum SendTo<'a> {
    CurSession(),
    UserId(&'a str),
    VideoHash(&'a str),
    MsgSender(&'a WsMsgSender),
}

/// Message from other server modules to user(s)
#[derive (Clone)]
pub struct UserMessage {
    pub user_id: String,
    pub msg: String,
    pub details: Option<String>,
}

#[derive (Clone)]
pub struct UploadResult {
    pub video_path: PathBuf,
    pub user_id: String,
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
        let serde_ser = serde_json::to_value(msg)?;
        self.emit_cmd("message", &serde_ser, SendTo::UserId(&msg.user_id)).map(|_| ())           
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
        self.emit_cmd("new_comment", &serde_json::to_value(c)? , send_to).map(|_| ())
    }

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
        tracing::error!("Error sending welcome message. Closing session. -- {}", e);
        return;
    }

    loop
    {
        tokio::select!
        {
            // Termination flag set? Exit.
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                if ses.server.terminate_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    tracing::info!("Termination flag set. Closing session.");
                    break;
             }},

            // Message in queue? Send to client.
            Some(msg) = msgq_rx.recv() => {
                tracing::debug!("[{}] Sending message to client: {:?}", sid, msg);
                if let Err(e) = ws_tx.send(msg).await {
                    tracing::error!("[{}] Error sending message - closing session: {}", sid, e);
                    break;
                }
            },

            // Message from client? Handle it.
            Some(msg) = ws_rx.next() => {
                match msg {
                    Err(e) => {
                        tracing::error!("[{}] Error receiving message - closing session: {}", sid, e);
                        break;
                    },
                    Ok(msg) => {
                        if msg.is_text() {

                            fn parse_msg(msg: &Message) -> Res<(String, serde_json::Value)> {
                                let msg_str = msg.to_str().unwrap_or("!!msg was supposed to .is_text()!!");
                                let json: serde_json::Value = serde_json::from_str(msg_str)?;
                                let cmd = json["cmd"].as_str().ok_or("Missing cmd")?.to_string();
                                let data = json.get("data").unwrap_or(&serde_json::json!({})).clone();
                                Ok((cmd, data))
                            }

                            let (cmd, data) = match parse_msg(&msg) {
                                Ok((cmd, data)) => (cmd, data),
                                Err(e) => {
                                    let answ = format!("Error parsing JSON message.");
                                    tracing::warn!("[{}] {}: {}", sid, answ, e);
                                    if !msgq_tx.send(Message::text(answ)).is_ok() { break; }
                                    continue;
                                }
                            };
                            tracing::debug!("[{}] Cmd '{}' from '{}'", sid, cmd, ses.user_id);

                            if let Err(e) = msg_dispatch(&cmd, &data, &mut ses).await {
                                    if let Some(e) = e.downcast_ref::<tokio::sync::mpsc::error::SendError<Message>>() {
                                        tracing::error!("[{}] Error sending message. Closing session. -- {}", sid, e);
                                        break;
                                    } else {
                                        let answ = format!("Error handling command '{}'.", cmd);
                                        tracing::warn!("[{}] {}: {}", sid, answ, e);
                                        if !msgq_tx.send(Message::text(answ)).is_ok() { break; };
                                    }
                                };

                        } else if msg.is_close() {
                            tracing::info!("[{}] Received close message", sid);
                            break
                        } else {
                            tracing::error!("[{}] Received unexpected message - closing session: {:?}", sid, msg);
                            break
                        };
                    }
                }
            },

            else => {
                if ses.sender.is_closed() {
                    tracing::info!("[{}] Message queue gone. Closing session.", sid);
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
                    Err(e) => tracing::warn!("Error parsing header '{}': {}", n, e),
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
    upload_results_tx: crossbeam_channel::Sender<UploadResult>,
    port: u16)
        -> Res<()>
{
    let session_counter = Arc::new(RwLock::new(0u32));
    let server_state_cln1 = server_state.clone();
    let server_state_cln2 = server_state.clone();

    tracing::info!("Starting API server on port {}", port);

    let rt_hello = warp::path("hello").map(|| "Hello, World!");

    let upload_dir = server_state.upload_dir.clone();
    let rt_upload = warp::path("upload")
        .and(warp::post())
        .and(warp::any().map(move || upload_dir.clone()))
        .and(warp::any().map(move || upload_results_tx.clone()))
        .and(warp::header::<mime::Mime>("content-type"))
        .and(warp::header::headers_cloned())
        .and(warp::body::stream())
        .and_then(handle_multipart_upload);

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
                format!("{}-{}", user_id, *counter)
            };

            let server_state = server_state.clone();
            ws.on_upgrade(|ws| async {
                // Diesel SQLite calls are blocking, so run a thread per user session
                // even though we're using async/await
                tokio::task::spawn_blocking( move || {
                    block_on(handle_ws_session(ws, sid, user_id, user_name, server_state));
                }).await.unwrap_or_else(|e| {
                    tracing::error!("Error joining handle_ws_session thread: {}", e); });
            })
        });

    let routes = rt_hello.or(rt_api_ws).or(rt_upload);
    let routes = routes.with(warp::log("api_server"));

    let routes = routes.with(warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST"])
        .allow_headers(vec!["Content-Type"]));

    let (_addr, server) = warp::serve(routes)
        .bind_with_graceful_shutdown(([127, 0, 0, 1], port), async move {
            while !server_state_cln1.terminate_flag.load(std::sync::atomic::Ordering::Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });

    let server_state = server_state_cln2;
    let msg_relay = async move {
        while !server_state.terminate_flag.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            while let Ok(m) = user_msg_rx.try_recv() {
                let msg = models::MessageInsert  {
                    event_name: "message".into(),
                    user_id: m.user_id.clone(),
                    message: m.msg,
                    details: m.details.unwrap_or("".into()),
                    seen: false, ref_comment_id: None, ref_video_hash: None,
                };
                if let Err(e) = server_state.db.add_message(&msg) {
                    tracing::error!("Failed to save user notification in DB: {:?}", e);
                }
                if let Ok(data) = serde_json::to_value(msg) {
                    let msg = Message::text(serde_json::json!({
                        "cmd": "message", "data": data }).to_string());
                    if let Err(_) = server_state.send_to_all_user_sessions(&m.user_id, &msg) {
                        tracing::error!("Failed to send user notification '{}'", m.user_id);
                    }
                }
            }
        };
    };

    tokio::join!(server, msg_relay);


    tracing::info!("API server stopped");
    Ok(())
}


#[tokio::main]
pub async fn run_forever(
    db: Arc<DB>,
    user_msg_rx: crossbeam_channel::Receiver<UserMessage>,
    upload_res_tx: crossbeam_channel::Sender<UploadResult>,
    terminate_flag: Arc<AtomicBool>,
    port: u16)
        -> Res<()>
{
    let state = ServerState::new( db,
        Path::new("DEV_DATADIR/videos"),
        Path::new("DEV_DATADIR/upload"),
        terminate_flag );
    run_api_server_async(state, user_msg_rx, upload_res_tx, port).await
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::connect_async;
    use crate::database::DB;

    #[tokio::test]
    async fn test_api_server_echo() {
        let terminate_flag = Arc::new(AtomicBool::new(false));
        let port = 13128;

        let db = Arc::new(DB::connect_db_url(":memory:").unwrap());
        let (_user_msg_tx, user_msg_rx) = crossbeam_channel::unbounded::<UserMessage>();
        let (upload_tx, _upload_rx) = crossbeam_channel::unbounded::<UploadResult>();

        let api_server_state = ServerState::new(
            db,
            Path::new("DEV_DATADIR/videos"),
            Path::new("DEV_DATADIR/upload"),
            terminate_flag.clone());
 
            let api_server = run_api_server_async(api_server_state, user_msg_rx, upload_tx, port);

        let testit = async move {
            let url = Url::parse("ws://127.0.0.1:13128/api/ws").unwrap();
            let (mut ws_stream, _) = connect_async(url).await.unwrap();
            ws_stream.send(Message::text("{\"cmd\": \"echo\", \"data\": \"hello\"}")).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let msg = ws_stream.next().await.unwrap().unwrap();
            assert!( msg.to_string().to_lowercase().contains("welcome"));

            let msg = ws_stream.next().await.unwrap().unwrap();
            assert_eq!(msg.to_string(), "Echo: hello");

            terminate_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        };

        let (res, _) = tokio::join!(api_server, testit);
        res.unwrap()
    }
}
