use std::path::Path;
use crate::grpc::grpc_client::{connect, OrganizerConnection};
use super::{grpc_client::OrganizerURI, grpc_server::BindAddr};
use super::proto;

pub struct OrganizerCaller {
    uri: OrganizerURI,
}

impl OrganizerCaller {
    pub fn new(uri: OrganizerURI ) -> Self {
        OrganizerCaller { uri }
    }
    
    pub fn handshake_organizer(&self, data_dir: &Path, server_url: &str, db_file: &Path, backchannel: &BindAddr)
        -> anyhow::Result<()>
    {
        async fn call_it(conn: &mut OrganizerConnection, backchannel: &BindAddr, data_dir: &Path, server_url: &str, db_file: &Path) -> anyhow::Result<()> {
            let v = semver::Version::parse(crate::PKG_VERSION)?;
            let req = proto::ServerInfo {
                storage: Some(proto::server_info::Storage {
                    storage: Some(proto::server_info::storage::Storage::LocalFs(
                        proto::server_info::storage::LocalFilesystem {
                            base_dir: data_dir.to_string_lossy().into()
                    }))}),
                backchannel: Some(proto::server_info::GrpcEndpoint {
                    endpoint: Some(
                        match backchannel {
                            BindAddr::Tcp(addr) => 
                                proto::server_info::grpc_endpoint::Endpoint::Tcp(
                                    proto::server_info::grpc_endpoint::Tcp {
                                        host: addr.ip().to_string(),
                                        port: addr.port() as u32,
                                    }),
                            BindAddr::Unix(path) =>
                                proto::server_info::grpc_endpoint::Endpoint::Unix(
                                    proto::server_info::grpc_endpoint::Unix {
                                        path: path.to_string_lossy().into(),
                                    }),
                        })
                    }),
                url_base: server_url.into(),
                db: Some(proto::server_info::Database {
                    r#type: proto::server_info::database::DatabaseType::Sqlite.into(),
                    endpoint: db_file.canonicalize()?.to_str().ok_or(
                        anyhow::anyhow!("Sqlite path is not valid UTF-8"))?.into()
                    }),
                version: Some(proto::SemanticVersionNumber { major: v.major, minor: v.minor, patch: v.patch }),
            };
            conn.handshake(req).await?;
            Ok(())
        }
        let (rt, mut conn) = self.tokio_connect()?;
        rt.block_on(call_it(&mut conn, backchannel, data_dir, server_url, db_file))
    }


    /// Helper for code that's not already async
    fn tokio_connect(&self) -> anyhow::Result<(tokio::runtime::Runtime, OrganizerConnection)> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let client = rt.block_on(connect(self.uri.clone()))?;
        Ok((rt, client))
    }    

}
