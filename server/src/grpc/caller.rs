use std::path::Path;
use lib_clapshot_grpc::GrpcBindAddr;

use crate::grpc::grpc_client::{connect, OrganizerConnection};
use super::grpc_client::OrganizerURI;
use super::proto;

pub struct OrganizerCaller {
    uri: OrganizerURI,
}

impl OrganizerCaller {
    pub fn new(uri: &OrganizerURI ) -> Self {
        OrganizerCaller { uri: uri.clone() }
    }

    pub fn blocking_handshake_organizer(&self, data_dir: &Path, server_url: &str, db_file: &Path, backchannel: &GrpcBindAddr)
        -> anyhow::Result<()>
    {
        async fn async_call_handshake(conn: &mut OrganizerConnection, backchannel: &GrpcBindAddr, data_dir: &Path, server_url: &str, db_file: &Path)
         -> anyhow::Result<()>
        {
            let v = semver::Version::parse(crate::PKG_VERSION)?;
            use lib_clapshot_grpc::proto::org as org;

            let req = proto::org::ServerInfo {
                storage: Some(org::server_info::Storage {
                    storage: Some(org::server_info::storage::Storage::LocalFs(
                        org::server_info::storage::LocalFilesystem {
                            base_dir: data_dir.to_string_lossy().into()
                    }))}),
                backchannel: Some(org::server_info::GrpcEndpoint {
                    endpoint: Some(
                        match backchannel {
                            GrpcBindAddr::Tcp(addr) =>
                            org::server_info::grpc_endpoint::Endpoint::Tcp(
                                org::server_info::grpc_endpoint::Tcp {
                                        host: addr.ip().to_string(),
                                        port: addr.port() as u32,
                                    }),
                            GrpcBindAddr::Unix(path) =>
                            org::server_info::grpc_endpoint::Endpoint::Unix(
                                org::server_info::grpc_endpoint::Unix {
                                        path: path.to_string_lossy().into(),
                                    }),
                        })
                    }),
                url_base: server_url.into(),
                db: Some(org::Database {
                    r#type: org::database::DatabaseType::Sqlite.into(),
                    endpoint: db_file.canonicalize()?.to_str().ok_or(
                        anyhow::anyhow!("Sqlite path is not valid UTF-8"))?.into()
                    }),
                version: Some(proto::org::SemanticVersionNumber { major: v.major, minor: v.minor, patch: v.patch }),
            };
            conn.handshake(req).await?;
            Ok(())
        }

        const MAX_TRIES: usize = 5;
        for retry in 1..(MAX_TRIES+1) {
            match self.tokio_connect() {
                Ok((rt, mut conn)) => {
                    tracing::info!("Connected to organizer (on attempt {retry}). Doing handshake.");
                    return rt.block_on(async_call_handshake(&mut conn, backchannel, data_dir, server_url, db_file));
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
    pub fn tokio_connect(&self) -> anyhow::Result<(tokio::runtime::Runtime, OrganizerConnection)> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let client = rt.block_on(connect(self.uri.clone()))?;
        Ok((rt, client))
    }

}
