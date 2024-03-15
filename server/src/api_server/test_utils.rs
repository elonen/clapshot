#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use std::path::PathBuf;
use tokio_tungstenite::tungstenite::Message;

use crate::video_pipeline::IncomingFile;
use crate::api_server::UserMessage;
use crate::database::{DB, models};

pub(crate) struct ApiTestState {
    pub(crate) db: Arc<DB>,
    pub(crate) user_msg_tx: crossbeam_channel::Sender<UserMessage>,
    pub(crate) upload_res_rx: crossbeam_channel::Receiver<IncomingFile>,
    pub(crate) videos_dir: PathBuf,
    pub(crate) upload_dir: PathBuf,
    pub(crate) terminate_flag: Arc<AtomicBool>,
    pub(crate) videos: Vec<models::Video>,
    pub(crate) comments: Vec<models::Comment>,
    pub(crate) url_base: String,
    pub(crate) port: u16,
    pub(crate) ws_url: String,
}

pub(crate) type WsClient = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub(crate) async fn read(ws: &mut WsClient) -> Option<String> {
    let res = match async_std::future::timeout(
        std::time::Duration::from_secs_f32(0.25), ws.next()).await {
            Ok(Some(m)) => Some(m.expect("Failed to read server message")).map(|m| m.to_string()),
            _ => None,
    };
    let res_str = res.as_ref().map(|s| s.as_str()).unwrap_or("<none>");
    println!("<--- [Client got]: {res_str}");
    res
}

pub(crate) async fn expect_msg(ws: &mut WsClient) -> String {
    read(ws).await.expect("Got no message from server")
}

pub(crate) async fn expect_cmd_data(ws: &mut WsClient) -> (serde_json::Value, serde_json::Value) {
    let msg = expect_msg(ws).await;
    let p: serde_json::Value = serde_json::from_str(&msg).expect("Failed to parse server message as JSON");
    (p["cmd"].clone(), p["data"].clone())
}

pub(crate) async fn read_cmd_data(ws: &mut WsClient) -> Option<(serde_json::Value, serde_json::Value)> {
    if let Some(msg) = read(ws).await {
        let p: serde_json::Value = serde_json::from_str(&msg).expect("Failed to parse server message as JSON");
        return Some((p["cmd"].clone(), p["data"].clone()));
    };
    None
}

pub(crate) async fn expect_no_msg(ws: &mut WsClient) {
    assert!(read(ws).await.is_none(), "Got unexpected message from server");
}

pub(crate) async fn write(ws: &mut WsClient, msg: &str) {
    println!("---> [Client sending]: {}", msg);
    ws.send(Message::text(msg)).await.expect("Failed to send WS message");
}

pub(crate) async fn connect_client_ws(ws_url: &str, user_id: &str) -> WsClient {
    use tokio_tungstenite::tungstenite::http;
    use tokio_tungstenite::connect_async;
    
    let request = http::Request::builder()
    .uri(ws_url)
    .header("Host", "127.0.0.1")
    .header("HTTP_X_REMOTE_USER_ID", user_id)
    .header("HTTP_X_REMOTE_USER_NAME", "User Num1")
    .header("Connection", "Upgrade")
    .header("Upgrade", "websocket")
    .header("Sec-WebSocket-Version", "13")
    .header("Sec-WebSocket-Key", "1234567890")    
    .body(()).unwrap();

    let (mut ws, _) = connect_async(request).await.unwrap();

    tracing::info!("TEST: Client connected. Waiting for 'welcome'...");
    assert!( expect_msg(&mut ws).await.to_lowercase().contains("welcome"));

    ws
}

macro_rules! api_test {
    ([$ws:ident, $state:ident] $($body:tt)*) => {
        {
            let (db, data_dir, videos, comments) = make_test_db();

            let port = portpicker::pick_unused_port().expect("No TCP ports free");
            let (user_msg_tx, user_msg_rx) = crossbeam_channel::unbounded();
            let (upload_res_tx, upload_res_rx) = crossbeam_channel::unbounded();
            let terminate_flag = Arc::new(AtomicBool::new(false));
            let url_base = format!("http://127.0.0.1:{port}");
            let ws_url = url_base.replace("http", "ws") + "/api/ws";
            let videos_dir = data_dir.join("videos");
            let upload_dir = data_dir.join("upload");
    
            let server_state = ServerState::new( db.clone(),
                &videos_dir.clone(),
                &upload_dir.clone(),
                &url_base.clone(),
                terminate_flag.clone());
    
            let $state = ApiTestState { db, user_msg_tx, upload_res_rx, videos_dir, upload_dir, terminate_flag, videos, comments, url_base, port, ws_url };
            let api = async move { run_api_server_async(server_state, user_msg_rx, upload_res_tx, port).await; Ok(()) };
            
            let tst = tokio::spawn(async move {
                tracing::info!("TEST: Client connecting to {}", $state.ws_url);
                #[allow(unused_mut)]
                let mut $ws = connect_client_ws(&$state.ws_url, "user.num1").await;
                tracing::info!("TEST: Running the tests...");
                { $($body)* }
                tracing::info!("TEST: Test finished. Terminating...");
                $state.terminate_flag.store(true, Relaxed);
            });
            tracing::info!("TEST: Test finished. Waiting for server to terminate...");
            tokio::try_join!(api, tst).unwrap();
        }
    }
}

/// Send an "open video" message to the server
/// 
/// # Arguments
/// * `ws` - WebSocket connection to the server
/// * `vh` - ID of the video to open
/// 
/// # Returns
/// * `("open_video", data)` - The command and data fields of the server response
pub(crate) async fn open_video(ws: &mut WsClient, vh: &str) -> (String, serde_json::Value)
{
    write(ws, &format!(r#"{{"cmd":"open_video","data":{{"video_hash":"{}"}}}}"#, vh)).await;
    let (cmd, data) = expect_cmd_data(ws).await;
    assert_eq!(cmd, "open_video");
    while let Some((cmt_cmd, cmt_data)) = read_cmd_data(ws).await {
        assert_eq!(cmt_cmd, "new_comment");
        assert_eq!(cmt_data["video_hash"], vh);
    }
    (cmd.to_string(), data)
}
