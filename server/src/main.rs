use anyhow::bail;
use clap::Parser;
use clapshot_server::{
    grpc::{grpc_client::prepare_organizer, grpc_server::make_grpc_server_bind},
    run_clapshot, PKG_NAME, PKG_VERSION,
};
use std::{path::PathBuf, sync::Arc};
use tracing::error;
use indoc::indoc;

mod log;

#[derive(Parser, Debug)]
#[command(
    name = PKG_NAME,
    version = PKG_VERSION,
    about = "Clapshot Server - backend of a media annotation tool",
    long_about = indoc! {"
        Clapshot Server - backend of a media annotation tool

        This is a small HTTP + WS server that listen to Client API requests,
        delegates some of them to an Organizer plugins, and transcodes media files.

        It monitors `<data_dir>/incoming` for new media files, processes them, and stores them in `<data_dir>/videos`.
        Use a proxy server to serve files from `videos` folder, and to secure the API with HTTPS/WSS.
        "},
)]
struct Args {
    /// Directory for database, /incoming, /videos and /rejected
    #[arg(short='D', long, required=true, value_name="DIR" )]
    data_dir: PathBuf,

    /// Base URL of the API server, e.g. `https://clapshot.example.com`.
    /// This depends on your proxy server, and is usually different from `--host` and `--port`.
    #[arg(short='U', long, required=true, value_name="URL")]
    url_base: String,


    /// TCP port to listen on
    #[arg(short='p', long, default_value_t = 8095)]
    port: u16,

    /// Host to listen on
    #[arg(short='H', long, default_value_t = String::from("127.0.0.1"))]
    host: String,

    /// Allowed CORS Origins, separated by commas.
    /// Defaults to the value of `url_base`.
    #[arg(long, value_name="ORIGINS")]
    cors: Option<String>,


    /// Polling interval for incoming folder
    #[arg(short='P', long, default_value_t = 3.0, value_name="SECONDS")]
    poll: f32,

    /// Max number of workers for media file processing
    /// (0 = number of CPU cores)
    #[arg(short, long, default_value_t = 0, value_name="NUM")]
    workers: usize,

    /// Target (max) bitrate for transcoding, in Mbps
    #[arg(short, long, default_value_t = 2.5, value_name="MBITS")]
    bitrate: f32,


    /// Migrate database to latest version. Makes an automatic backup.
    #[arg(long)]
    migrate: bool,


    /// Log to file instead of stdout
    #[arg(short, long, value_name="FILE")]
    log: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Log in JSON format
    #[arg(short, long)]
    json: bool,


    /// Use this user id if auth headers are not found.
    /// Mainly useful for debugging.
    #[arg(long, default_value = "anonymous", value_name="USER")]
    default_user: String,


    /// Shell command to start Organizer plugin.
    /// The command should block until SIGTERM, and log to stdout/stderr without timestamps.
    /// Unless --org-uri is a HTTP(S) URI, the command will get a Unix socket path as an argument when Clapshot server calls it.
    #[arg(long, value_name="CMD")]
    org_cmd: Option<String>,    // TODO: turn into a Vec<String> to allow multiple plugins

    /// Custom endpoint for srv->org connections.
    /// E.g. `/path/to/plugin.sock` or `http://[::1]:50051`
    /// If `--org-cmd` is given, this defaults to a temp .sock in datadir.
    #[arg(long, value_name="URI")]
    org_in_uri: Option<String>,

    /// Listen in TCP address port for org->srv connections.
    /// Default is to use a Unix socket in datadir. E.g. `[::1]:50052`
    #[arg(long, value_name="BIND")]
    org_out_tcp: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.bitrate < 0.1 {
        bail!("Bitrate must be >= 0.1");
    }
    let target_bitrate = (args.bitrate * 1_000_000.0) as u32;

    if !args.data_dir.exists() {
        bail!("Data directory does not exist: {:?}", args.data_dir);
    }

    let url_base = args.url_base.trim_end_matches('/').to_string();
    let time_offset = time::UtcOffset::current_local_offset().expect("should get local offset");

    let _logger = Arc::new(log::ClapshotLogger::new(
        time_offset,
        args.debug,
        &args.log.clone().unwrap_or_default(),
        args.json,
    )?);

    let grpc_server_bind = make_grpc_server_bind(&args.org_out_tcp, &args.data_dir)?;

    let (org_uri, _org_hdl) = prepare_organizer(
        &args.org_in_uri,
        &args.org_cmd,
        args.debug,
        args.json,
        &args.data_dir,
    )?;

    let cors_origins: Vec<String> = args.cors
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let default_user = args.default_user.clone();

    // Run the server (blocking)
    if let Err(e) = run_clapshot(
        args.data_dir.to_path_buf(),
        args.migrate,
        url_base,
        cors_origins,
        args.host,
        args.port,
        org_uri,
        grpc_server_bind,
        if args.workers == 0 { num_cpus::get() } else { args.workers },
        target_bitrate,
        default_user,
        args.poll,
        args.poll * 5.0,
    ) {
        error!("run_clapshot() failed: {}", e);
    }

    Ok(())
}