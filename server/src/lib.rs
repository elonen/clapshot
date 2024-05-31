use std::{collections::HashMap, path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Arc}, thread::{self, JoinHandle}};

use anyhow::Context;
use database::{db_backup::{backup_sqlite_database, restore_sqlite_database}, migration_solver::MigrationGraphModule, sqlite_foreign_key_check, DB};
use lib_clapshot_grpc::{proto::org::{self, Migration}, GrpcBindAddr};
use crate::{api_server::server_state::ServerState, grpc::{caller::OrganizerCaller, grpc_client::OrganizerURI}};

use anyhow::bail;

pub mod video_pipeline;
pub mod api_server;
pub mod database;
pub mod tests;
pub mod grpc;

pub const PKG_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const PKG_NAME: &'static str = env!("CARGO_PKG_NAME");

const SERVER_MODULE_NAME: &str = "clapshot.server";     // Name for migrations solver


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

        let _span = tracing::info_span!("INIT").entered();

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
        let db_was_missing = !db_file.exists();

        if migrate || db_was_missing {
            migrate_db(&db_file, &organizer_uri)?;
        }

        let grpc_srv_listening_flag = Arc::new(AtomicBool::new(false));
        let db: Arc<DB> = Arc::new(database::DB::open_db_file(&db_file).unwrap());

        // Run API server
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

        // Handshake Organizer if configured
        match &organizer_uri {
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
                let org = OrganizerCaller::new(&ouri);
                tracing::info!("Connecting gRPC srv->org...");
                org.blocking_handshake_organizer(&data_dir, &url_base, &db_file, &grpc_server_bind)?;
                tracing::debug!("srv->org handshake done.");
            }
            None => {
                tracing::debug!("No Organizer URI provided, skipping gRPC.");
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



/// Find migrations from server and organizer, solve their dependencies, and apply them.
/// Backup before starting, and restore if foreign key checks fail after applying the migrations.
fn migrate_db( db_file: &PathBuf, org_uri: &Option<OrganizerURI>) -> anyhow::Result<()>
{
    use lib_clapshot_grpc::proto::org::CheckMigrationsRequest;
    let _span = tracing::info_span!("migrate_db").entered();

    let db: Arc<DB> = Arc::new(database::DB::open_db_file(&db_file).context("Error opening DB file")?);
    let cur_server_migration = db.latest_applied_server_migration_name()?;
    let pending_server_migrations = db.pending_server_migrations()?;

    match db.conn().and_then(|mut conn| { sqlite_foreign_key_check(&mut conn, false) }) {
        Err(_) => tracing::warn!("^^^ Foreign key checks failed BEFORE migrations. Migrations might correct these, but if not, fix the DB and try again."),
        Ok(())=> tracing::debug!("Foreign key checks Ok before migrations."),
    }

    // Represent server migrations for solver
    let server_module = {
        let mut prev_ver: Option<String> = cur_server_migration.clone();
        let server_migs = pending_server_migrations.iter().map(|(m_name, m_version)| {
            let mig = Migration {
                uuid: m_name.clone(),
                version: m_version.clone(),
                dependencies: vec![lib_clapshot_grpc::proto::org::migration::Dependency {
                    name: SERVER_MODULE_NAME.to_string(),
                    min_ver: prev_ver.clone(),
                    max_ver: prev_ver.clone(),
                }],
                description: "".to_string(),
            };
            prev_ver = Some(m_version.clone());
            mig
        }).collect::<Vec<_>>();

        tracing::debug!("Clapshot server has {} pending migrations.", server_migs.len());
        MigrationGraphModule {
            name: SERVER_MODULE_NAME.to_string(),
            cur_version: cur_server_migration.clone(),
            migrations: server_migs
        }
    };

    let mut migration_modules: Vec<MigrationGraphModule> = vec![ server_module ];

    let org_db_info = Some(org::Database {
        r#type: org::database::DatabaseType::Sqlite.into(),
        endpoint: db_file.canonicalize()?.to_str().ok_or(
            anyhow::anyhow!("Sqlite path is not valid UTF-8"))?.into()
    });

    // Add Organizer and its migrations, if available
    if let Some(uri) = org_uri {
        let caller = OrganizerCaller::new(uri);
        let (rt, mut org_conn) = caller.tokio_connect().context("Error connecting to Organizer")?;
        tracing::debug!("Calling check_migrations on Organizer.");

        match rt.block_on(org_conn.check_migrations(CheckMigrationsRequest { db: org_db_info.clone() })) {
            Ok(cm_res) => {
                let migrations = cm_res.get_ref().pending_migrations.clone();
                tracing::debug!("Organizer has {} pending migrations.", migrations.len());
                migration_modules.push(MigrationGraphModule {
                    name: cm_res.get_ref().name.clone(),
                    cur_version: Some(cm_res.get_ref().current_schema_ver.clone()),
                    migrations,
                });
            }
            Err(e) => {
                match e.code() {
                    tonic::Code::NotFound => { tracing::info!("No pending migrations from Organizer."); },
                    tonic::Code::Unimplemented => { tracing::info!("Organizer does not implement migrations. Ignoring."); },
                    _ => { anyhow::bail!("Error checking Organizer migrations: {:?}", e); }
                }
            }
        }
    };

    match migration_modules.iter().map(|m| m.migrations.len()).sum::<usize>() {
        0 => {
            tracing::info!("No pending migrations.");
            return Ok(());
        },
        n => { tracing::debug!("Total {} migrations to consider. Solving dependencies.", n); }
    }

    // Solve migration order
    let migration_order = database::migration_solver::solve_migration_graph(migration_modules.iter().collect())?;
    match migration_order {
        None => {
            tracing::error!("Failed to solve migration dependencies. List of considered migrations:");
            for m in migration_modules {
                tracing::error!("Module: '{}': current version: '{:?}'", &m.name, &m.cur_version);
                for mig in &m.migrations {
                    tracing::error!("  - '{}', brings version to '{}'  depends on: '{:?}')", mig.uuid, mig.version, mig.dependencies);
                }
            }
            bail!("Cannot proceed with migrations due to unsolvable dependencies.");
        },
        // Solver returned a list of migrations to apply
        Some(order) => {
            if order.is_empty() {
                tracing::info!("Empty plan. No migrations to apply.");
                return Ok(());
            }

            tracing::info!("Migration plan created.");
            drop(db);   // Close before backup
            let db_backup_file = backup_sqlite_database(db_file.into())?;

            let db: Arc<DB> = Arc::new(database::DB::open_db_file(&db_file).context("Error opening DB file")?);
            match apply_migrations(&migration_modules, &order, &db, db_file, org_uri,org_db_info.clone())
                .and_then(|_| { db.conn().context("Error opening DB connection after migrations") })
                .and_then(|mut conn| { sqlite_foreign_key_check(&mut conn, true).context("Foreign key checks failed after migrations") })
            {
                Ok(_) => {
                    tracing::info!("Migrations applied successfully. Foreign keys checked Ok.");
                    return Ok(());
                },
                Err(e) => {
                    drop (db);  // Close before restore
                    tracing::error!(error=%e, "Migration failure. Restoring DB from the backup.");
                    match db_backup_file {
                        None => {
                            tracing::warn_span!("No backup file found. Skipping restore. This usually means DB was missing before migrations. If that's the case, delete the dangling DB before trying again.");
                        },
                        Some(db_backup_file) => {
                            restore_sqlite_database(db_file.into(), db_backup_file)
                                .context("Error restoring DB after failed migrations")?;

                            tracing::info!("DB restored.");
                            bail!("Migration(s) failed: {:?}", e);
                        }
                    }
                }
            }
        },
    }

    Ok(())
}


/// Execute given migration plan.
fn apply_migrations(
    migration_modules: &Vec<MigrationGraphModule>,
    plan: &Vec<Migration>,
    db: &Arc<DB>,
    db_file: &PathBuf,
    org_uri: &Option<OrganizerURI>,
    org_db_info: Option<org::Database>
) -> Result<(), anyhow::Error>
{
    use lib_clapshot_grpc::proto::org::ApplyMigrationRequest;

    let uuid_to_mod: HashMap<String, String> = migration_modules.iter().flat_map(|m| {
        m.migrations.iter().map(|mig| (mig.uuid.clone(), m.name.clone()))
    }).collect();

    for mig in plan {
        match uuid_to_mod.get(mig.uuid.as_str()) {
            // Server
            Some(module_name) if module_name == SERVER_MODULE_NAME => {
                let mut conn = db.conn()?;
                if let Err(e) = db.apply_server_migration(&mut conn, &mig.uuid) {
                    tracing::error!(file=%db_file.display(), err=?e, "Error applying migration '{}'. Rolling back.", &mig.uuid);
                    bail!("Error applying migration '{}'", &mig.uuid);
                }
            }
            // Organizer
            Some(module_name) => {
                let _span = tracing::info_span!("apply org migration", name=mig.uuid, new_ver=mig.version, org=module_name).entered();
                tracing::info!("Applying on Organizer...");

                if let Some(uri) = org_uri {
                    let (rt, mut org_conn) = OrganizerCaller::new(uri).tokio_connect()
                        .context("Error connecting to organizer for migrations")?;
                    rt.block_on(org_conn.apply_migration(ApplyMigrationRequest {
                        db: org_db_info.clone(),
                        uuid: mig.uuid.clone()
                    })).map_err(|e| anyhow::anyhow!("Error applying organizer migration '{}': {:?}", &mig.uuid, e))?;
                } else {
                    bail!("Organizer migration '{}' found but no organizer URI to connect to.", &mig.uuid);
                }
            },
            None => {
                bail!("Migration '{}' not found in modules. This should not happen.", mig.uuid);
            }
        }
    }
    Ok(())
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