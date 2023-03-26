use std::path::PathBuf;
use anyhow::Context;
use serde::Deserialize;
use docopt::Docopt;
use simple_organizer::{NAME, VERSION};
use lib_clapshot_grpc::proto;
use tokio::sync::mpsc;
use tracing::info;
mod log;

const USAGE: &'static str = r#"
{NAME} {VERSION}

Default/example Clapshot Organizer plugin.
This gRPC server can bind to Unix socket or TCP address.

Usage:
  {NAME} [options] [--tcp] <bind>
  
  {NAME} (-h | --help)
  {NAME} (-v | --version)

Required:
    <bind>              Unix socket or IP address to bind to.
                        e.g. '/tmp/organizer.sock' or '[::1]:50051'

    --tcp               Bind to a TCP port instead of Unix socket.

Options:
 -j --json              Log in JSON format
 -d --debug             Enable debug logging
 -h --help              Show this screen
 -v --version           Show version
"#;

#[derive(Debug, Deserialize)]
struct Args {
    arg_bind: String,
    flag_tcp: bool,
    flag_json: bool,
    flag_debug: bool,
    flag_version: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()>
{    
    let args: Args = Docopt::new(USAGE.replace("{NAME}", NAME).replace("{VERSION}", VERSION))
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    if args.flag_version {
        println!("{}", VERSION);
        return Ok(());
    }
    
    log::setup_logging(args.flag_json, args.flag_debug)?;

    tracing::info!("Organizer plugin '{}' v{} starting up...", NAME, VERSION);
    run_grpc_server(
        if args.flag_tcp {
            BindAddr::Tcp(args.arg_bind)
        } else {
            BindAddr::Unix(PathBuf::from(args.arg_bind))
        }
    ).await
}


#[derive(Debug)]
enum BindAddr {
    Tcp(String),
    Unix(PathBuf),
}

async fn run_grpc_server(bind: BindAddr) -> anyhow::Result<()>
{
    tracing::info!("srv->org gRPC server: Binding to '{:?}'", bind);

    use tonic::{transport::Server};
    use proto::organizer_inbound_server::OrganizerInboundServer;
    use simple_organizer::SimpleOrganizer;

    let refl = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(proto::FILE_DESCRIPTOR_SET)
        .build()?;

    let srv = Server::builder()
        .add_service(refl)
        .add_service(OrganizerInboundServer::new(SimpleOrganizer::default()));

    let (shutdown_send, mut shutdown_recv) = mpsc::unbounded_channel();
    ctrlc::set_handler(move || { shutdown_send.send(()).unwrap(); })
        .expect("Error setting Ctrl-C handler");

    let wait_for_shutdown = async move { shutdown_recv.recv().await; };

    match bind {
        BindAddr::Tcp(addr) => {
                srv.serve_with_shutdown(addr.parse()?, wait_for_shutdown).await?;
        },
        BindAddr::Unix(path) => {
            if path.try_exists()? {
                std::fs::remove_file(&path).context("Failed to delete previous socket.")?;
            }
            srv.serve_with_incoming_shutdown(
                    tokio_stream::wrappers::UnixListenerStream::new(
                        tokio::net::UnixListener::bind(&path)?),
                    wait_for_shutdown
                ).await?;
        }
    }
    info!("Exiting gracefully.");
    Ok(())
}
