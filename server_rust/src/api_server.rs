use warp::Filter;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool};
// use std::path::{PathBuf};
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use warp::ws::{Message};

type Res<T> = std::result::Result<T, Box<dyn std::error::Error>>;


async fn ws_echo(ws: warp::ws::WebSocket ) {
    let (mut tx, mut rx) = ws.split();
    while let Some(result) = rx.next().await {
        let msg = result.unwrap();
        let res = Message::text(format!("echo: {}", msg.to_str().unwrap()));
        tx.send(res).await.unwrap();
    }
}

async fn run_api_server_async(terminate_flag: Arc<AtomicBool>, port: u16) -> Res<()>
{
    tracing::info!("Starting API server on port {}", port);

    let rt_hello = warp::path("hello").map(|| "Hello, World!");

    let rt_api_ws = warp::path("api").and(warp::path("ws"))
        .and(warp::ws())
        .map(|ws: warp::ws::Ws| {
            ws.on_upgrade(ws_echo)
        });

    let routes = rt_hello.or(rt_api_ws);
    let routes = routes.with(warp::log("api_server"));

    let routes = routes.with(warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST"])
        .allow_headers(vec!["Content-Type"]));

    let (_addr, server) = warp::serve(routes)
        .bind_with_graceful_shutdown(([127, 0, 0, 1], port), async move {
            while !terminate_flag.load(std::sync::atomic::Ordering::Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });
    server.await;
    tracing::info!("API server stopped");
    Ok(())
}


#[tokio::main]
pub async fn run_forever(terminate_flag: Arc<AtomicBool>, port: u16) -> Res<()> {
    run_api_server_async(terminate_flag, port).await
}


#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::connect_async;

    #[tokio::test]
    async fn test_api_server_echo() {
        let terminate_flag = Arc::new(AtomicBool::new(false));
        let port = 13128;

        let api_server = run_api_server_async(terminate_flag.clone(), port);

        let testit = async move {
            let url = Url::parse("ws://127.0.0.1:13128/api/ws").unwrap();
            let (mut ws_stream, _) = connect_async(url).await.unwrap();
            ws_stream.send(Message::text("hello")).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let msg = ws_stream.next().await.unwrap().unwrap();
            assert_eq!(msg.to_string(), "echo: hello");
            terminate_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        };

        let (res, _) = tokio::join!(api_server, testit);
        res.unwrap()
    }
}
