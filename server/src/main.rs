use docopt::Docopt;
use serde::Deserialize;
use tracing::{error};
use std::{path::{PathBuf}};
use anyhow::{bail};
use clapshot_server::{PKG_NAME, PKG_VERSION, run_clapshot, grpc::{grpc_client::prepare_organizer, grpc_server::parse_server_bind}};

mod log;

const USAGE: &'static str = r#"
Clapshot server - backend of a video annotation tool

Monitors <path>/incoming for new videos, processes them, and stores them in <path>/videos.
Then serves the annotations and comments via an asyncronous HTTP + Socket.IO API.
Use a proxy server to serve files in /videos and to secure the API with HTTPS/WSS.

Usage:
{NAME} [options] (--url-base <url>) (--data-dir <path>)
{NAME} (-h | --help)

Required:

 --url-base <url>     Base URL of the API server, e.g. https://example.com/clapshot/.
                      This depends on your proxy server configuration.
 --data-dir <path>    Directory for database, /incoming, /videos and /rejected

Options:

 -p --port <port>     Port to listen on [default: 8095]
 -H --host <host>     Host to listen on [default: 127.0.0.1]

 -P --poll <sec>      Polling interval for incoming folder [default: 3.0]

 -l --log <file>      Log to file instead of stdout
 -j --json            Log in JSON format
 -w --workers <n>     Max number of workers for video processing [default: 0]
                      (0 = number of CPU cores)

 -b --bitrate <vbr>   Target (max) bitrate for transcoding, in Mbps [default: 2.5]
 --migrate            Migrate database to latest version. Make a backup first.

 -d --debug           Enable debug logging
 -h --help            Show this screen
 -v --version         Show version and exit

Organizer:

Plugin system for organizing videos and users. Clapshot server can connect to
a custom Organizer through gRPC, which then connects back to the server,
establishing bidirectional gRPC. Recommended way to run an Organizer
is to use --org-cmd, which allows integrated logging and shutdown.
Unix sockets are used by default, and you don't usually need to
specify --org-in-uri or --org-out-tcp.

 --org-cmd <cmd>       Shell command to start Organizer plugin.
                       The command should block until SIGTERM, and log to
                       stdout/stderr without timestamps. Unless --org-uri is a HTTP(S) URI,
                       the command will get a Unix socket path as an argument when
                       Clapshot server calls it.

 --org-in-uri <uri>    Custom endpoint for srv->org connections.
                       E.g. `/path/to/plugin.sock` or `http://[::1]:50051`
                       If `--org-cmd` is given, this defaults to a temp .sock in datadir.

 --org-out-tcp <addr>  Listen in TCP address port for org->srv connections.
                       Default is to use a Unix socket in datadir. E.g. `[::1]:50052`

"#;

#[derive(Debug, Deserialize)]
struct Args {
    flag_port: u16,
    flag_host: String,
    flag_poll: f32,
    flag_log: Option<String>,
    flag_json: bool,
    flag_workers: usize,
    flag_bitrate: f32,
    flag_migrate: bool,
    flag_org_cmd: Option<String>,
    flag_org_in_uri: Option<String>,
    flag_org_out_tcp: Option<String>,
    flag_url_base: String,
    flag_data_dir: PathBuf,
    flag_debug: bool,
    flag_version: bool,
}

fn main() -> anyhow::Result<()>
{
    let args: Args = Docopt::new(USAGE.replace("{NAME}", PKG_NAME))
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    if args.flag_version { println!("{}", PKG_VERSION); return Ok(()); }
    // if args.flag_debug { println!("Debug enabled; parsed command line: {:#?}", args); }

    let target_bitrate = {
        if args.flag_bitrate < 0.1 { bail!("Bitrate must be >= 0.1"); }
        (args.flag_bitrate * 1_000_000.0) as u32
    };

    if !(&args.flag_data_dir).exists() {
        bail!("Data directory does not exist: {:?}", args.flag_data_dir);
    }

    let url_base = args.flag_url_base.strip_suffix("/").unwrap_or("").to_string(); // strip trailing slash, if any

    let time_offset = time::UtcOffset::current_local_offset().expect("should get local offset");
    let _log_guard = log::setup_logging(
        time_offset,
        args.flag_debug,
        &args.flag_log.clone().unwrap_or_default(),
        args.flag_json);

    let grpc_server_bind = parse_server_bind(&args.flag_org_out_tcp, &args.flag_data_dir)?;

    let (org_uri, _org_hdl) = prepare_organizer(
        &args.flag_org_in_uri,
        &args.flag_org_cmd,
        &args.flag_data_dir)?;

    // Run the server (blocking)
    if let Err(e) =  run_clapshot(
        args.flag_data_dir.to_path_buf(),
        args.flag_migrate,
        url_base,
        args.flag_host,
        args.flag_port,
        org_uri,
        grpc_server_bind,
        if args.flag_workers == 0 { num_cpus::get() } else { args.flag_workers },
        target_bitrate,
        args.flag_poll,
        args.flag_poll * 5.0)
    {
        error!("run_clapshot() failed: {}", e);
    }

    Ok(())
}
