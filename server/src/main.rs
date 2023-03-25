use docopt::Docopt;
use std::path::PathBuf;
use anyhow::bail;
mod log;

const USAGE: &'static str = r#"
Clapshot server - backend of a video annotation tool

Monitors <path>/incoming for new videos, processes them, and stores them in <path>/videos.
Then serves the annotations and comments via an asyncronous HTTP + Socket.IO API.
Use a proxy server to serve files in /videos and to secure the API with HTTPS/WSS.

Usage:
  clapshot-server [options] (--url-base=URL) (--data-dir=PATH)
  clapshot-server [options] [--mute TOPIC]... (--url-base=URL) (--data-dir=PATH)
  clapshot-server (-h | --help)

Required:
 --url-base=URL       Base URL of the API server, e.g. https://example.com/clapshot/.
                      This depends on your proxy server configuration.
 --data-dir=PATH      Directory for database, /incoming, /videos and /rejected

Options:
 -p PORT --port=PORT    Port to listen on [default: 8095]
 -H HOST --host=HOST    Host to listen on [default: 0.0.0.0]
 --host-videos          Host the /videos directory
                        (For debugging. Use Nginx or Apache with auth in production.)
 -P SEC --poll SEC      Polling interval for incoming folder [default: 3.0]
 -m TOPIC --mute TOPIC    Mute logging for a topic (can be repeated). Sets level to WARNING.
                        See logs logs for available topics.
 -l FILE --log FILE     Log to file instead of stdout
 -j --json              Log in JSON format
 -w N --workers N       Max number of workers for video processing [default: 0]
                        (0 = number of CPU cores)
 -b VBR --bitrate VBR   Target (max) bitrate for transcoding, in Mbps [default: 2.5]
 --migrate              Migrate database to latest version. Make a backup first.

 -d --debug             Enable debug logging
 -h --help              Show this screen
"#;

fn main() -> anyhow::Result<()>
{
    let argv = std::env::args;
    //let argv = || vec!["clapshot-server", "--bitrate", "8", "--migrate", "--debug", "--url-base", "http://127.0.0.1:8095", "--data-dir", "DEV_DATADIR/"];

    let args = Docopt::new(USAGE)
        .and_then(|d| d.argv(argv().into_iter()).parse())
        .unwrap_or_else(|e| e.exit());

    let port_str = args.get_str("--port");
    let port = port_str.parse::<u16>().unwrap();

    let debug: bool = args.get_bool("--debug");
    let data_dir = PathBuf::from(args.get_str("--data-dir"));

    let log_file = args.get_str("--log").to_string();
    let json_log = args.get_bool("--json");

    let mut n_workers = args.get_str("--workers").parse::<usize>().unwrap_or(0);
    if n_workers == 0 { n_workers = num_cpus::get(); }

    let bitrate_mbps = args.get_str("--bitrate").parse::<f32>().unwrap_or(2.5);
    if bitrate_mbps < 0.1 { bail!("Bitrate must be >= 0.1"); }
    let target_bitrate = (bitrate_mbps * 1_000_000.0) as u32;

    let url_base = args.get_str("--url-base").to_string()
        .strip_suffix("/").unwrap_or("").to_string(); // strip trailing slash, if any

    let migrate = args.get_bool("--migrate");

    let poll_interval = args.get_str("--poll").parse::<f32>().unwrap_or(3.0);
    let resubmit_delay = poll_interval * 5.0;

    // Setup logging
    let time_offset = time::UtcOffset::current_local_offset().expect("should get local offset");
    let _log_guard = log::setup_logging(
        time_offset,
        debug,
        &log_file,
        json_log);

    clapshot_server::run_clapshot(data_dir, migrate, url_base, port, n_workers, target_bitrate, poll_interval, resubmit_delay)
}
