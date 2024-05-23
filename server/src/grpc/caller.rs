use std::path::Path;
use lib_clapshot_grpc::proto::org::{ApplyMigrationRequest, CheckMigrationsRequest, AfterMigrationsRequest};
use lib_clapshot_grpc::GrpcBindAddr;

use crate::grpc::grpc_client::{connect, OrganizerConnection};
use super::grpc_client::OrganizerURI;
use super::proto;

pub struct OrganizerCaller {
    uri: OrganizerURI,
}

impl OrganizerCaller {
    pub fn new(uri: OrganizerURI ) -> Self {
        OrganizerCaller { uri }
    }

    pub fn handshake_organizer(&self, data_dir: &Path, server_url: &str, db_file: &Path, backchannel: &GrpcBindAddr, cur_server_migration: Option<&str>)
        -> anyhow::Result<()>
    {
        async fn call_it(conn: &mut OrganizerConnection, backchannel: &GrpcBindAddr, data_dir: &Path, server_url: &str, db_file: &Path, cur_server_migration: Option<&str>)
         -> anyhow::Result<()>
        {
            let v = semver::Version::parse(crate::PKG_VERSION)?;
            use lib_clapshot_grpc::proto::org::server_info;
            let req = proto::org::ServerInfo {
                storage: Some(server_info::Storage {
                    storage: Some(server_info::storage::Storage::LocalFs(
                        server_info::storage::LocalFilesystem {
                            base_dir: data_dir.to_string_lossy().into()
                    }))}),
                backchannel: Some(server_info::GrpcEndpoint {
                    endpoint: Some(
                        match backchannel {
                            GrpcBindAddr::Tcp(addr) =>
                                server_info::grpc_endpoint::Endpoint::Tcp(
                                    server_info::grpc_endpoint::Tcp {
                                        host: addr.ip().to_string(),
                                        port: addr.port() as u32,
                                    }),
                            GrpcBindAddr::Unix(path) =>
                                server_info::grpc_endpoint::Endpoint::Unix(
                                    server_info::grpc_endpoint::Unix {
                                        path: path.to_string_lossy().into(),
                                    }),
                        })
                    }),
                url_base: server_url.into(),
                db: Some(server_info::Database {
                    r#type: server_info::database::DatabaseType::Sqlite.into(),
                    endpoint: db_file.canonicalize()?.to_str().ok_or(
                        anyhow::anyhow!("Sqlite path is not valid UTF-8"))?.into()
                    }),
                version: Some(proto::org::SemanticVersionNumber { major: v.major, minor: v.minor, patch: v.patch }),
            };
            conn.handshake(req).await?;

            // TODO: Handle organizer migrations properly
            // This is a naive version that doesn't handle dependencies at all.
            tracing::info!("Calling check_migrations on organizer.");

            match conn.check_migrations(CheckMigrationsRequest {}).await {
                Ok(cm_res) => {
                    let mut pending = cm_res.get_ref().pending_migrations.clone();

                    // Apply migrations
                    pending.sort_by(|a, b| a.version.cmp(&b.version));  // Oldest first
                    let mut cur_version = cm_res.get_ref().current_schema_ver.clone();
                    for m in pending {
                        tracing::warn!("MIGRATION DEPENDENCY RESOLVER NOT YET PROPERLY IMPLEMENTED! Doing rudimentary checks and applying organizer migration: {:?}", m);

                        assert!(m.version > cur_version, "Migration version {} is not greater than current version {} -- this needs a better implementation to resolve", m.version, cur_version);
                        for dep in &m.dependencies {
                            if dep.name == "clapshot.server" {
                                if let Some(min_ver) = &dep.min_ver {
                                    assert!(cur_server_migration.unwrap_or_default() >= min_ver.as_str(), "Migration '{}' requires server DB version >= {} but server is at version {}", m.uuid, min_ver, cur_server_migration.unwrap_or_default());
                                }
                                if let Some(max_ver) = &dep.max_ver {
                                    assert!(cur_server_migration.unwrap_or_default() <= max_ver.as_str(), "Migration '{}' requires server DB version <= {} but server is at version {}", m.uuid, max_ver, cur_server_migration.unwrap_or_default());
                                }
                            }
                        }
                        conn.apply_migration(ApplyMigrationRequest { uuid: m.uuid.clone() }).await
                            .map_err(|e| anyhow::anyhow!("Error applying organizer migration '{}': {:?}", m.uuid, e))?;
                        cur_version = m.version;
                    }
                },
                Err(e) => {
                    match e.code() {
                        tonic::Code::NotFound => { tracing::info!("No migrations found on organizer."); },
                        tonic::Code::Unimplemented => { tracing::info!("Organizer does not implement migrations. Ignoring."); },
                        _ => { anyhow::bail!("Error checking organizer migrations: {:?}", e); }
                    }
                }
            };

            if let Err(e) = conn.after_migrations(AfterMigrationsRequest {}).await {
                if e.code() == tonic::Code::Unimplemented {
                    tracing::info!("Organizer does not implement after_migrations. Ignoring.");
                } else {
                    anyhow::bail!("Error calling after_migrations on organizer: {:?}", e);
                }
            }

            Ok(())
        }

        const MAX_TRIES: usize = 5;
        for retry in 1..(MAX_TRIES+1) {
            match self.tokio_connect() {
                Ok((rt, mut conn)) => {
                    tracing::info!("Connected to organizer (on attempt {retry}). Doing handshake.");
                    return rt.block_on(call_it(&mut conn, backchannel, data_dir, server_url, db_file, cur_server_migration));
                },
                Err(e) => {
                    tracing::warn!("Connecting organizer failed (attempt {retry}/{MAX_TRIES}: {}", e);
                    std::thread::sleep(std::time::Duration::from_secs_f32(0.5));
                }
            }
        }
        anyhow::bail!("Connecting organizer failed after {MAX_TRIES} attempts");
    }


    /// Helper for code that's not already async
    fn tokio_connect(&self) -> anyhow::Result<(tokio::runtime::Runtime, OrganizerConnection)> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let client = rt.block_on(connect(self.uri.clone()))?;
        Ok((rt, client))
    }

}
