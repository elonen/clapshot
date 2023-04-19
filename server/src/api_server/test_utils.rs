#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use lib_clapshot_grpc::proto;
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
    pub(crate) nodes: Vec<models::PropNode>,
    pub(crate) edges: Vec<models::PropEdge>,
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
    if let Some(Some(res_json)) = res.as_ref().map(|s| serde_json::from_str::<serde_json::Value>(s).ok()) {
        println!("<--- [Client got]: {:#}", res_json);
    } else {
        println!("<--- [Client got]: {:#}", res_str);
    }
    res
}
pub(crate) async fn expect_msg(ws: &mut WsClient) -> String {
    read(ws).await.expect("Got no message from server")
}

pub(crate) async fn expect_parsed<T>(ws: &mut WsClient) -> T
    where T: serde::de::DeserializeOwned
{
    let msg = expect_msg(ws).await;
    serde_json::from_str::<T>(&msg).expect(format!("Failed to parse type '{}' message from JSON", std::any::type_name::<T>()).as_str())
}

pub(crate) async fn try_get_parsed<T>(ws: &mut WsClient) -> Option<T>
    where T: serde::de::DeserializeOwned
{
    if let Some(msg) = read(ws).await {
        Some(serde_json::from_str::<T>(&msg).expect(format!("Failed to parse type '{}' message from JSON", std::any::type_name::<T>()).as_str()))
    } else {
        None
    }
}

#[macro_export]
macro_rules! expect_client_cmd {
    ($ws:expr, $variant:ident) => {{
        println!("Expecting client command '{}'...", stringify!($variant));
        match crate::api_server::test_utils::expect_parsed::<proto::client::ServerToClientCmd>($ws).await.cmd {
            Some(lib_clapshot_grpc::proto::client::server_to_client_cmd::Cmd::$variant(v)) => {
                println!("...got '{}' ok.", stringify!($variant));
                println!(". . .");
                v
            },
            _ => panic!("Expected client command '{}' BUT GOT SOMETHING ELSE.", stringify!($variant)),
        }
    }}
}

/*
pub(crate) async fn expect_cmd_data(ws: &mut WsClient) -> (serde_json::Value, serde_json::Value) {
    let msg = expect_msg(ws).await;
    let p: serde_json::Value = serde_json::from_str(&msg).expect("Failed to parse server message as JSON");
    (p["cmd"].clone(), p["data"].clone())
}
*/

/*
pub(crate) async fn read_cmd_data(ws: &mut WsClient) -> Option<(serde_json::Value)> {
    if let Some(msg) = read(ws).await {
        let p: serde_json::Value = serde_json::from_str(&msg).expect("Failed to parse server message as JSON");
        return Some((p["cmd"].clone(), p["data"].clone()));
    };
    None
}
*/

pub (crate) async fn wait_for_thumbnails(ws: &mut WsClient) {
    println!("Waiting for thumbnail generation...");
    let mut thumb_done = false;
    for _ in 0..12 {
        match crate::api_server::test_utils::try_get_parsed::<proto::client::ServerToClientCmd>(ws).await
        .map(|c| c.cmd).flatten() {
            Some(proto::client::server_to_client_cmd::Cmd::ShowMessages(m)) => {
                if m.msgs[0].r#type == proto::user_message::Type::VideoUpdated as i32 {
                    thumb_done = true;
                    break;
                } else {
                    println!("  (... got some other message: {:?})", m.msgs[0]);
                }
            },
            None => {
                // Wait for file to be processed
                tokio::time::sleep(Duration::from_secs_f32(0.2)).await;
            },
            _ => panic!("Unexpected message while waitig for thumbnail generation"),
        }
    };
    if !thumb_done {
        panic!("... thumbnail generation TIMED OUT");
    }
    println!("...thumbnail notification received.");
    println!(". . .");
}

pub(crate) async fn expect_no_msg(ws: &mut WsClient) {
    assert!(read(ws).await.is_none(), "Got unexpected message from server");
}

pub(crate) async fn write(ws: &mut WsClient, msg: &str) {
    println!("---> [Client sending]: {:#}", msg);
    ws.send(Message::text(msg)).await.expect("Failed to send WS message");
}

pub(crate) async fn connect_client_ws(ws_url: &str, user_id: &str) -> WsClient {
    use tokio_tungstenite::tungstenite::http;
    use tokio_tungstenite::connect_async;

    let request = http::Request::builder()
        .uri(ws_url.clone())
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
    expect_client_cmd!(&mut ws, Welcome);

    tracing::info!("TEST: Client got 'welcome'. Waiting for 'defineActions'...");
    expect_client_cmd!(&mut ws, DefineActions);

    ws
}

macro_rules! api_test {
    ([$ws:ident, $state:ident] $($body:tt)*) => {
        {
            let (db, data_dir, videos, comments, nodes, edges) = make_test_db();

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
                None,
                terminate_flag.clone());

            let bind_addr: std::net::IpAddr = "127.0.0.1".parse().unwrap();
            let $state = ApiTestState { db, user_msg_tx, upload_res_rx, videos_dir, upload_dir, terminate_flag, videos, comments, nodes, edges, url_base, port, ws_url };
            let api = async move { run_api_server_async(bind_addr, server_state, user_msg_rx, upload_res_tx, None, port).await; Ok(()) };

            let tst = tokio::spawn(async move {
                tracing::info!("TEST: Client connecting to {}", $state.ws_url);
                #[allow(unused_mut)]
                let mut $ws = connect_client_ws(&$state.ws_url, "user.num1").await;
                println!("TEST: Running the tests...");
                { $($body)* }
                println!("TEST: Test finished. Terminating...");
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
/// * `vid` - ID of the video to open
///
/// # Returns
/// * OpenVideo message from the server
pub(crate) async fn open_video(ws: &mut WsClient, vid: &str) -> proto::client::server_to_client_cmd::OpenVideo
{
    println!("--------- TEST: open_video '{}'...", vid);
    write(ws, &format!(r#"{{"cmd":"open_video","data":{{"id":"{}"}}}}"#, vid)).await;
    let ov = expect_client_cmd!(ws, OpenVideo);

    while let Some(msg) = read(ws).await {
        let cmd: proto::client::ServerToClientCmd = serde_json::from_str(&msg).expect("Failed to parse ServerToClientCmd from JSON");
        match cmd.cmd {
            // Make sure the comments are for the video we opened
            Some(proto::client::server_to_client_cmd::Cmd::AddComments(m)) => {
                assert!(m.comments.iter().all(|c| c.video_id == vid));
            },
            // Thumbnail generation can take a while, so ignore it if it happens to be in the queue
            Some(proto::client::server_to_client_cmd::Cmd::ShowMessages(m)) => {
                assert!(m.msgs.iter().any(|m| m.message.contains("thumbnail")));
            },
            None => {},
            _ => panic!("Unexpected message from server: {}", msg),
        }
    };
    ov
}
