use std::{fs::File, sync::{atomic::{AtomicBool, Ordering}, Arc}, thread::{self, JoinHandle}};

use anyhow::Context;
use database::DB;
use diesel::Connection;
use flate2::{write::GzEncoder, Compression};
use lib_clapshot_grpc::GrpcBindAddr;
use crate::{api_server::server_state::ServerState, grpc::{caller::OrganizerCaller, grpc_client::OrganizerURI}};

pub mod video_pipeline;
pub mod api_server;
pub mod database;
pub mod tests;
pub mod grpc;

pub const PKG_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const PKG_NAME: &'static str = env!("CARGO_PKG_NAME");


pub struct ClapshotInit {
    terminate_flag: Arc<AtomicBool>,
    api_thread: Option<JoinHandle<()>>,
    vpp_thread: Option<JoinHandle<()>>,
}

impl ClapshotInit {

    /// Initialize clapshot and spawn all worker threads.
    pub fn init_and_spawn_workers(
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
        default_user: String,
        resubmit_delay: f32,
        terminate_flag: Arc<AtomicBool>)
        -> anyhow::Result<Self>
    {
        use signal_hook::consts::TERM_SIGNALS;
        use signal_hook::flag;
        use crossbeam_channel::unbounded;   // Work queue

        for sig in TERM_SIGNALS {
            flag::register_conditional_shutdown(*sig, 1, Arc::clone(&terminate_flag))?;
            flag::register(*sig, Arc::clone(&terminate_flag))?;
        }

        // Create subdirectories
        for d in &["videos", "incoming", "videos"] {
            std::fs::create_dir_all(&data_dir.join(d))?;
        }

        // Initialize database
        let db_file = data_dir.join("clapshot.sqlite");
        let db = connect_and_migrate_db(db_file.clone(), migrate)?;

        // Run API server
        let grpc_srv_listening_flag = Arc::new(AtomicBool::new(false));
        let (user_msg_tx, user_msg_rx) = unbounded::<api_server::UserMessage>();
        let (upload_tx, upload_rx) = unbounded::<video_pipeline::IncomingFile>();
        let api_thread = Some({
            let server = ServerState::new( db.clone(),
                &data_dir.join("videos"),
                &data_dir.join("upload"),
                &url_base,
                organizer_uri.clone(),
                grpc_srv_listening_flag.clone(),
                default_user,
                terminate_flag.clone());
            let grpc_srv = if (&organizer_uri).is_some() { Some(grpc_server_bind.clone()) } else { None };
            let ub = url_base.clone();
            thread::spawn(move || { api_server::run_forever(user_msg_rx, grpc_srv, upload_tx, bind_api.to_string(), ub, cors_origins, server, port) })
        });

        // Connect to organizer if configured
        match organizer_uri.clone() {
            Some(ouri) => {
                // Wait for our gRPC server thread to bind before handshaking with organizer
                let start_time = std::time::Instant::now();
                while !grpc_srv_listening_flag.load(Ordering::Relaxed) {
                    thread::sleep(std::time::Duration::from_millis(10));
                    if start_time.elapsed().as_secs() > 3 {
                        anyhow::bail!("gRPC server failed to start within 3 seconds.");
                    }
                }
                // Ok, organizer should be able to connect back to us now, so handshake
                let org = OrganizerCaller::new(ouri);
                tracing::info!("Connecting gRPC srv->org...");
                org.handshake_organizer(&data_dir, &url_base, &db_file, &grpc_server_bind)?;
                tracing::info!("srv->org handshake done (org->srv not connected yet).");
            }
            None => {
                tracing::info!("No Organizer URI provided, skipping gRPC.");
            }
        };

        // Run media file processing pipeline
        let tf = Arc::clone(&terminate_flag);
        let dd = data_dir.clone();
        let vpp_thread = Some({
            let db = db.clone();
            thread::spawn(move || { video_pipeline::run_forever(
                db, tf.clone(), dd, user_msg_tx, poll_interval, resubmit_delay, target_bitrate, upload_rx, n_workers)})
        });


        Ok(ClapshotInit {terminate_flag, api_thread, vpp_thread})
    }


    /// Block until the terminate flag is set
    pub fn wait_for_termination(&mut self) -> anyhow::Result<()>
    {
        // Loop forever, abort on SIGINT/SIGTERM or if child threads die
        while !self.terminate_flag.load(Ordering::Relaxed) {
            thread::sleep(std::time::Duration::from_secs(1));
            if self.vpp_thread.as_mut().map_or(true, |t| t.is_finished()) {
                self.terminate_flag.store(true, Ordering::Relaxed);
            }
        }

        tracing::info!("Got kill signal. Cleaning up.");
        self.vpp_thread.take().and_then(|t| t.join().ok()).expect("VPP thread failed");
        self.api_thread.take().and_then(|t| t.join().ok()).expect("API thread failed");
        Ok(())
    }
}


/// Connect to the database and run migrations if needed.
///
/// If `migrate` is true, run migrations automatically,
/// otherwise bail with an error if migrations are needed.
fn connect_and_migrate_db( db_file: std::path::PathBuf, migrate: bool ) -> anyhow::Result<Arc<DB>>
{
    use anyhow::bail;

    let db_was_missing = !db_file.exists();
    let db = Arc::new(database::DB::open_db_file(&db_file).unwrap());

    let pending_migrations = db.pending_migration_names()?;

    // Check & apply database migrations
    if  (migrate || db_was_missing) && !pending_migrations.is_empty() {
        if db_file.exists() {
            // Make a tar.gz backup
            let now = chrono::Local::now();
            let backup_path = db_file.with_extension(format!("backup-{}.tar.gz", now.format("%Y-%m-%dT%H_%M_%S")));
            tracing::info!(file=%db_file.display(), backup=%backup_path.display(), "Backing up database before migration.");

            let backup_file = File::create(&backup_path).context("Error creating DB backup file")?;
            let gzip_writer = GzEncoder::new(backup_file, Compression::fast());
            let mut tar_builder = tar::Builder::new(gzip_writer);

            // Add the main .sqlite file to the tar archive
            tar_builder.append_path_with_name(&db_file, db_file.file_name().unwrap())
                .context("Error adding main DB file to tar archive")?;

            // Add .sqlite-* files to the tar archive
            let db_file_prefix = db_file.to_string_lossy().into_owned();
            for entry in std::fs::read_dir(db_file.parent().unwrap()).context("Error reading DB directory")? {
                let entry = entry.context("Error reading DB directory entry")?;
                let path = entry.path();
                if path.to_string_lossy().starts_with(&db_file_prefix) && path != db_file {
                    tar_builder.append_path_with_name(&path, path.file_name().unwrap())
                        .context("Error adding WAL/SHM files to tar archive")?;
                }
            }

            tar_builder.finish().context("Error finishing tar archive")?;
        }

        for m in &pending_migrations {
            db.conn()?.transaction::<(), _, _>(|conn| {
                if let Err(e) = db.apply_migration(conn, m) {
                    tracing::error!(file=%db_file.display(), "Error applying migration '{}'. Rolling back.", m);
                    bail!("Error applying migration {}: {:?}", m, e);
                };
                Ok(())
            })?;
        }
        tracing::info!(file=%db_file.display(), "All server migrations applied.");

    } else {
        if !pending_migrations.is_empty() {
            eprintln!("Database migrations needed. Run `clapshot-server --migrate`");
            std::process::exit(1);
        } else {
            tracing::info!(file=%db_file.display(), "No database migrations needed.");
        }
    }
    Ok(db)
}



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
    default_user: String,
    poll_interval: f32,
    resubmit_delay: f32
) -> anyhow::Result<()> {

    let terminate_flag = Arc::new(AtomicBool::new(false));

    // Initialize clapshot
    let mut clapshot = ClapshotInit::init_and_spawn_workers(
        data_dir,
        migrate,
        url_base,
        cors_origins,
        bind_api,
        port,
        organizer_uri,
        grpc_server_bind,
        n_workers,
        target_bitrate,
        poll_interval,
        default_user,
        resubmit_delay,
        terminate_flag.clone()
    )?;

    // Wait until termination
    clapshot.wait_for_termination()
}