#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use docopt::Docopt;
use std::thread;
use std::io::Error;
use std::path::{PathBuf};

// For termination signals
use std::sync::atomic::{AtomicBool, Ordering};
use signal_hook::consts::TERM_SIGNALS;
use signal_hook::flag;
use std::sync::Arc;

use crossbeam_channel::unbounded;   // Work queue

// For logging
use tracing::{info, error, warn};
use tracing_subscriber::FmtSubscriber;

use clapshot_server::database;
use clapshot_server::api_server;
use clapshot_server::video_pipeline;


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
 -w N --workers N       Max number of workers for video processing [default: 0]
                        (0 = number of CPU cores)
 -b VBR --bitrate VBR   Target (max) bitrate for transcoding, in Mbps [default: 2.5]
 --migrate              Migrate database to latest version. Make a backup first.

 -d --debug             Enable debug logging
 -h --help              Show this screen
"#;

fn main() -> Result<(), Box<dyn std::error::Error>>
{
    let argv = || vec!["clapshot-server", "--bitrate", "8", "--migrate", "--debug", "--url-base", "http://127.0.0.1:8095", "--data-dir", "DEV_DATADIR/"];

    let args = Docopt::new(USAGE)
        .and_then(|d| d.argv(argv().into_iter()).parse())
        .unwrap_or_else(|e| e.exit());

    let port_str = args.get_str("--port");
    let port = port_str.parse::<u16>().unwrap();

    let debug: bool = args.get_bool("--debug");
    let data_dir = PathBuf::from(args.get_str("--data-dir"));

    let mut n_workers = args.get_str("--workers").parse::<usize>().unwrap_or(0);
    if n_workers == 0 { n_workers = num_cpus::get(); }

    let bitrate_mbps = args.get_str("--bitrate").parse::<f32>().unwrap_or(2.5);
    if bitrate_mbps < 0.1 { return Err("Bitrate must be >= 0.1".into()); }
    let target_bitrate = (bitrate_mbps * 1_000_000.0) as u32;

    let url_base = args.get_str("--url-base").to_string();


    // Setup logging
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "debug,clapshot_server=debug");
    };
    let log_sbsc = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .compact() // for production
        //.pretty() // for debugging
        .with_file(false)
        .with_line_number(false)
        .with_thread_ids(true)
        .with_target(true)
        .finish();

    tracing::subscriber::set_global_default(log_sbsc).expect("tracing::subscriber::set_global_default failed");
    tracing::debug!("Debug logging enabled");

    // Setup SIGINT / SIGTERM handling
    let terminate_flag = Arc::new(AtomicBool::new(false));
    for sig in TERM_SIGNALS {
        flag::register_conditional_shutdown(*sig, 1, Arc::clone(&terminate_flag))?;
        flag::register(*sig, Arc::clone(&terminate_flag))?;
    }

    let db_file = data_dir.join("clapshot.sqlite");
    let db = Arc::new(database::DB::connect_db_file(&db_file).unwrap());

    // Check & apply database migrations
    if args.get_bool("--migrate") && db.migrations_needed()? {
        match db.run_migrations() {
            Ok(_) => {
                assert!(!db.migrations_needed()?);
                tracing::warn!("Database migrated Ok. Continuing.");
            },
            Err(e) => { return Err("Error migrating database".into()); },
        }
    } else {
        match db.migrations_needed() {
            Ok(false) => {},
            Ok(true) => {
                eprintln!("Database migrations needed. Make a backup and run `clapshot-server --migrate`");
                std::process::exit(1);
            },
            Err(e) => { return Err("Error checking database migrations".into()); },
        }
    }

    // Run API server
    let tf = Arc::clone(&terminate_flag);
    let (user_msg_tx, user_msg_rx) = unbounded::<api_server::UserMessage>();
    let (upload_tx, upload_rx) = unbounded::<video_pipeline::IncomingFile>();
    let api_thread = { 
        let db = db.clone();
        thread::spawn(move || {
            if let Err(e) = api_server::run_forever(db, user_msg_rx, upload_tx, tf.clone(), url_base.to_string(), port) {
                error!("API server failed: {}", e);
                tf.store(true, Ordering::Relaxed);
            }})};

    // Run video processing pipeline
    let tf = Arc::clone(&terminate_flag);
    let vpp_thread = {
            let db = db.clone();
            thread::spawn(move || { video_pipeline::run_forever(
                db, tf.clone(), data_dir, user_msg_tx, 3.0, 15.0, target_bitrate, upload_rx, n_workers)})
        };

    // Loop forever, abort on SIGINT/SIGTERM or if child threads die
    while !terminate_flag.load(Ordering::Relaxed)
    {
        thread::sleep(std::time::Duration::from_secs(1));
        if vpp_thread.is_finished() {
            tracing::info!("Video pipeline thread finished.");
            terminate_flag.store(true, Ordering::Relaxed);
        }
    }

    tracing::warn!("Got kill signal. Cleaning up.");
    vpp_thread.join().unwrap();

    tracing::warn!("Cleanup done. Exiting.");
    Ok(())
}

