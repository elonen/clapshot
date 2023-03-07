use crate::grpc::{connect::{OrganizerURI}, caller::OrganizerCaller};

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
    bind_addr: String,
    port: u16,
    organizer_uri: Option<OrganizerURI>,
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
                eprintln!("Database migrations needed. Make a backup and run `clapshot-server --migrate`");
                std::process::exit(1);
            },
            Err(e) => { bail!("Error checking database migrations: {:?}", e); },
        }
    }

    if let Some(ouri) = organizer_uri {
        OrganizerCaller::new(ouri).server_started(&data_dir, &url_base, &db_file)?;
    }

    // Run API server
    let tf = Arc::clone(&terminate_flag);
    let (user_msg_tx, user_msg_rx) = unbounded::<api_server::UserMessage>();
    let (upload_tx, upload_rx) = unbounded::<video_pipeline::IncomingFile>();
    let api_thread = { 
        let db = db.clone();
        let data_dir = data_dir.clone();
        thread::spawn(move || {
            api_server::run_forever(
                    db,
                    data_dir.join("videos"),
                    data_dir.join("upload"),
                    user_msg_rx, 
                    upload_tx, 
                    tf.clone(), 
                    bind_addr.to_string(),
                    url_base.to_string(),
                    port) 
            })};

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
