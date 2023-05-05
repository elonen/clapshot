use anyhow::Context;
use lib_clapshot_grpc::GrpcBindAddr;
use crate::{grpc::{grpc_client::{OrganizerURI}, caller::OrganizerCaller}, api_server::server_state::ServerState};

pub mod video_pipeline;
pub mod api_server;
pub mod database;
pub mod tests;
pub mod grpc;

pub const PKG_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const PKG_NAME: &'static str = env!("CARGO_PKG_NAME");


pub fn run_clapshot(
    data_dir: std::path::PathBuf,
    migrate: bool,
    url_base: String,
    cors_origins: Vec<String>,
    bind_api: String,
    port: u16,
    organizer_uri: Option<OrganizerURI>,
    grpc_server_bind: GrpcBindAddr,
    n_workers: usize,
    target_bitrate: u32,
    poll_interval: f32,
    resubmit_delay: f32)
        -> anyhow::Result<()>
{
    use std::thread;
    use std::sync::atomic::{AtomicBool, Ordering};
    use signal_hook::consts::TERM_SIGNALS;
    use signal_hook::flag;
    use std::sync::Arc;
    use anyhow::bail;

    use crossbeam_channel::unbounded;   // Work queue

    // Setup SIGINT / SIGTERM handling
    let terminate_flag = Arc::new(AtomicBool::new(false));
    for sig in TERM_SIGNALS {
        flag::register_conditional_shutdown(*sig, 1, Arc::clone(&terminate_flag))?;
        flag::register(*sig, Arc::clone(&terminate_flag))?;
    }

    // Create subdirectories
    for d in &["videos", "incoming", "videos"] {
        std::fs::create_dir_all(&data_dir.join(d))?;
    }

    let db_file = data_dir.join("clapshot.sqlite");
    let was_missing = !db_file.exists();
    if was_missing {
        eprintln!("Database file not found, running migrations to create it.");
    }
    let db = Arc::new(database::DB::connect_db_file(&db_file).unwrap());

    // Check & apply database migrations
    if  (migrate || was_missing) && db.migrations_needed()? {
        if !was_missing {
            // Make a gzipped backup
            let now = chrono::Local::now();
            let backup_path = db_file.with_extension(format!("backup-{}.sqlite.gz", now.format("%Y-%m-%dT%H_%M_%S")));
            tracing::warn!(file=%db_file.display(), backup=%backup_path.display(), "Backing up database before migration.");
            let backup_file = std::fs::File::create(&backup_path).context("Error creating DB backup file")?;
            let mut gzip_writer = flate2::write::GzEncoder::new(backup_file, flate2::Compression::fast());
            let mut fh = std::fs::File::open(&db_file).context("Error reading current DB file for backup")?;
            std::io::copy(&mut fh, &mut gzip_writer).context("Error copying DB file to backup")?;
        }
        match db.run_migrations() {
            Ok(_) => {
                assert!(!db.migrations_needed()?);
                tracing::warn!(file=%db_file.display(), "Database migrated Ok. Continuing.");
            },
            Err(e) => { bail!("Error migrating database: {:?}", e); },
        }
    } else {
        match db.migrations_needed() {
            Ok(false) => {},
            Ok(true) => {
                eprintln!("Database migrations needed. Run `clapshot-server --migrate`");
                std::process::exit(1);
            },
            Err(e) => { bail!("Error checking database migrations: {:?}", e); },
        }
    }

    // Run API server
    let (user_msg_tx, user_msg_rx) = unbounded::<api_server::UserMessage>();
    let (upload_tx, upload_rx) = unbounded::<video_pipeline::IncomingFile>();
    let api_thread = {
        let db = db.clone();
        let data_dir = data_dir.clone();

        let server = ServerState::new( db,
            &data_dir.join("videos"),
            &data_dir.join("upload"),
            &url_base,
            organizer_uri.clone(),
            terminate_flag.clone());

        let grpc_srv = if (&organizer_uri).is_some() { Some(grpc_server_bind.clone()) } else { None };
        let ub = url_base.clone();
        thread::spawn(move || {
            api_server::run_forever(
                    user_msg_rx,
                    grpc_srv,
                    upload_tx,
                    bind_api.to_string(),
                    ub,
                    cors_origins,
                    server,
                    port)
            })
        };

    match organizer_uri.clone() {
        Some(ouri) => {
            tracing::info!("Connecting gRPC srv->org...");
            OrganizerCaller::new(ouri).handshake_organizer(&data_dir, &url_base, &db_file, &grpc_server_bind)?;
            tracing::info!("Bidirectional gRPC established.");
        }
        None => {
            tracing::info!("No organizer URI provided, skipping gRPC.");
        }
    };

    // Run video processing pipeline
    let tf = Arc::clone(&terminate_flag);
    let vpp_thread = {
            let db = db.clone();
            thread::spawn(move || { video_pipeline::run_forever(
                db, tf.clone(), data_dir, user_msg_tx, poll_interval, resubmit_delay, target_bitrate, upload_rx, n_workers)})
        };

    // Loop forever, abort on SIGINT/SIGTERM or if child threads die
    while !terminate_flag.load(Ordering::Relaxed) {
        thread::sleep(std::time::Duration::from_secs(1));
        if vpp_thread.is_finished() {
            terminate_flag.store(true, Ordering::Relaxed);
        }
    }

    tracing::info!("Got kill signal. Cleaning up.");
    vpp_thread.join().unwrap();
    api_thread.join().unwrap();
    Ok(())
}
