use anyhow::bail;
use clap::Parser;
use clapshot_server::{
    api_server::validate_org_http_headers_regex,
    grpc::{grpc_client::prepare_organizer, grpc_server::make_grpc_server_bind},
    run_clapshot, PKG_NAME, PKG_VERSION,
    video_pipeline::IngestUsernameFrom,
};
use std::{path::PathBuf, sync::Arc, str::FromStr};
use tracing::error;
use indoc::indoc;

mod log;

/// Validate that a required script exists at the given path.
/// Returns an error with a helpful message if the script is not found.
fn validate_script_exists(path: &str, name: &str) -> anyhow::Result<()> {
    if !std::path::Path::new(path).exists() {
        bail!(
            "{} script not found: '{}'\n\
            If you upgraded the package and kept your old config file, you may need to add \
            the new '{}' setting. See docs for examples.",
            name, path, name.to_lowercase().replace(" ", "-")
        );
    }
    Ok(())
}

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

    /// Set debug level by repeating (-d = debug, -dd = trace)
    #[arg(short, long, action=clap::ArgAction::Count)]
    debug: u8,

    // Enable debug mode (same as -v)

    /// Log in JSON format
    #[arg(short, long)]
    json: bool,


    /// Use this user id if auth headers are not found.
    /// Mainly useful for debugging.
    #[arg(long, default_value="anonymous", value_name="USER")]
    default_user: String,

    /// How to determine username for files in incoming/ folder.
    /// 'file-owner' uses filesystem ownership, 'folder-name' uses first subfolder name.
    #[arg(long, default_value="file-owner", value_name="METHOD")]
    ingest_username_from: String,


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

    /// Path to custom transcoding script
    #[arg(long, value_name="SCRIPT", default_value="scripts/clapshot-transcode")]
    transcode_script: String,

    /// Path to custom thumbnailing script
    #[arg(long, value_name="SCRIPT", default_value="scripts/clapshot-thumbnail")]
    thumbnail_script: String,

    /// Path to custom transcoding decision script
    #[arg(long, value_name="SCRIPT", default_value="scripts/clapshot-transcode-decision")]
    transcode_decision_script: String,

    /// Path to an external notification hook script (optional).
    #[arg(long, value_name="SCRIPT")]
    notification_script: Option<String>,

    /// Comma-separated glob allowlist of notification events to deliver.
    /// Only has effect together with --notification-script. Default '*' = all events.
    #[arg(long, value_name="GLOBS", default_value="*")]
    notification_events: String,

    /// Regular expression to filter HTTP headers passed to Organizer.
    /// Only headers matching this pattern are included in UserSessionData. Case-insensitive.
    /// Default '^$' = disabled (none forwarded).
    ///
    /// SECURITY: the Organizer can't tell a header your auth layer set from one the client sent.
    /// Every name matched here MUST also be overridden (set or cleared) in the reverse proxy's
    /// /api location, or a client can spoof it. Prefer anchored exact names over loose prefixes,
    /// e.g. '^X[-_]REMOTE[-_]USER[-_]CAN[-_]UPLOAD$' rather than '^X[-_]REMOTE[-_]'.
    #[arg(long, value_name="REGEX", default_value="^$")]
    org_http_headers: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.bitrate < 0.1 {
        bail!("Bitrate must be >= 0.1");
    }
    let target_bitrate = (args.bitrate * 1_000_000.0) as u32;

    let ingest_username_from = IngestUsernameFrom::from_str(&args.ingest_username_from)
        .map_err(|e| anyhow::anyhow!("Invalid --ingest-username-from: {}", e))?;

    if !args.data_dir.exists() {
        bail!("Data directory does not exist: {:?}", args.data_dir);
    }

    // Validate that all required scripts exist
    validate_script_exists(&args.transcode_script, "Transcode")?;
    validate_script_exists(&args.thumbnail_script, "Thumbnail")?;
    validate_script_exists(&args.transcode_decision_script, "Transcode-decision")?;
    if let Some(ref s) = args.notification_script {
        validate_script_exists(s, "Notification")?;
    }

    let url_base = args.url_base.trim_end_matches('/').to_string();
    let time_offset = time::UtcOffset::current_local_offset().expect("should get local offset");

    let log_level = match args.debug {
        0 => tracing::Level::INFO,
        1 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };

    let _logger = Arc::new(log::ClapshotLogger::new(
        time_offset,
        log_level,
        &args.log.clone().unwrap_or_default(),
        args.json,
    )?);

    let grpc_server_bind = make_grpc_server_bind(&args.org_out_tcp, &args.data_dir)?;

    let (org_uri, _org_hdl) = prepare_organizer(
        &args.org_in_uri,
        &args.org_cmd,
        log_level,
        args.json,
        &args.data_dir,
    )?;

    let cors_origins: Vec<String> = args.cors
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let default_user = args.default_user.clone();

    // Validate and compile the org_http_headers regex
    let org_http_headers_regex = validate_org_http_headers_regex(&args.org_http_headers)?;

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
        ingest_username_from,
        args.transcode_script,
        args.thumbnail_script,
        args.transcode_decision_script,
        args.notification_script,
        args.notification_events,
        org_http_headers_regex,
    ) {
        error!("run_clapshot() failed: {}", e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_script_exists_fails_for_missing_script() {
        let result = validate_script_exists("/nonexistent/path/to/script", "Test-script");
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found"));
    }

    #[test]
    fn test_validate_script_exists_succeeds_for_existing_file() {
        // Use a file that definitely exists
        let result = validate_script_exists("/bin/sh", "Shell");
        assert!(result.is_ok());
    }
}