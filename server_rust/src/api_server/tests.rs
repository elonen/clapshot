#[cfg(test)]
mod tests
{
    #![allow(dead_code)]

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool};
    use futures_util::stream::StreamExt;
    use futures_util::SinkExt;
    use std::sync::atomic::Ordering::Relaxed;
    use std::path::{PathBuf};        
    use rand;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::connect_async;

    use crate::video_pipeline::IncomingFile;
    use crate::api_server::{UserMessage, run_api_server_async};
    use crate::api_server::server_state::ServerState;
    use crate::database::DB;

    struct ApiTestState {
        db: Arc<DB>,
        user_msg_tx: crossbeam_channel::Sender<UserMessage>,
        upload_res_rx: crossbeam_channel::Receiver<IncomingFile>,
        videos_dir: PathBuf,
        upload_dir: PathBuf,
        terminate_flag: Arc<AtomicBool>,
        url_base: String,
        port: u16,
        ws_url: String,
    }

    type WsClient = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    async fn read(ws: &mut WsClient) -> Option<String> {
        match async_std::future::timeout(
            std::time::Duration::from_secs_f32(0.1), ws.next()).await {
                Ok(Some(m)) => Some(m.expect("Failed to read server message")).map(|m| m.to_string()),
                _ => None,
        }
    }

    async fn expect_msg(ws: &mut WsClient) -> String {
        read(ws).await.expect("Got no message from server")
    }

    async fn expect_no_msg(ws: &mut WsClient) {
        assert!(read(ws).await.is_none(), "Got unexpected message from server");
    }

    async fn write(ws: &mut WsClient, msg: &str) {
        ws.send(Message::text(msg)).await.expect("Failed to send WS message");
    }

    macro_rules! api_test {
        ([$ws:ident, $state:ident] $($body:tt)*) => {
            {
                let port = 10000 + (rand::random::<u16>() % 10000);
                let db = Arc::new(DB::connect_db_url(":memory:").unwrap());
                let (user_msg_tx, user_msg_rx) = crossbeam_channel::unbounded();
                let (upload_res_tx, upload_res_rx) = crossbeam_channel::unbounded();
                let terminate_flag = Arc::new(AtomicBool::new(false));
                let url_base = format!("http://127.0.0.1:{port}");
                let ws_url = url_base.replace("http", "ws") + "/api/ws";
                let data_dir = assert_fs::TempDir::new().unwrap();
                let videos_dir = data_dir.join("videos");
                let upload_dir = data_dir.join("upload");
        
                let server_state = ServerState::new( db.clone(),
                    &videos_dir.clone(),
                    &upload_dir.clone(),
                    &url_base.clone(),
                    terminate_flag.clone());
        
                let $state = ApiTestState { db, user_msg_tx, upload_res_rx, videos_dir, upload_dir, terminate_flag, url_base, port, ws_url };
                let api = async move { run_api_server_async(server_state, user_msg_rx, upload_res_tx, port).await; Ok(()) };
        
                let tst = tokio::spawn(async move {
                    let (mut $ws, _) = connect_async($state.ws_url.clone()).await.unwrap();
                    assert!( expect_msg(&mut $ws).await.to_lowercase().contains("welcome"));
                    { $($body)* }
                    $state.terminate_flag.store(true, Relaxed);
                });
                tokio::try_join!(api, tst).unwrap();
            }
        }
    }

    // ---------------------------------------------------------------------------------------------

    #[tokio::test]
    async fn it_answers_echo()
    {
        api_test! {[ws, _ts] 
            write(&mut ws, r#"{"cmd":"echo","data":"hello"}"#).await;
            assert_eq!(expect_msg(&mut ws).await, "Echo: hello");
        }
    }
}
