use std::path::Path;
use crate::grpc::connect::{connect, OrganizerConnection, proto};
use super::connect::OrganizerURI;

pub struct OrganizerCaller {
    uri: OrganizerURI
}

impl OrganizerCaller {
    pub fn new(uri: OrganizerURI) -> Self {
        OrganizerCaller { uri }
    }  
    
    pub fn server_started(&self, data_dir: &Path, server_url: &str, db_file: &Path)
        -> anyhow::Result<()>
    {
        async fn call_it(conn: &mut OrganizerConnection, data_dir: &Path, server_url: &str, db_file: &Path) -> anyhow::Result<()> {
            let v = semver::Version::parse(crate::PKG_VERSION)?;
            let req = proto::ServerInfo {
                data_dir: data_dir.to_string_lossy().into(),
                server_url: server_url.into(),
                db: Some(proto::DatabaseInfo {
                    r#type: proto::database_info::DatabaseType::Sqlite.into(),
                    endpoint: db_file.canonicalize()?.to_str().ok_or(
                        anyhow::anyhow!("Sqlite path is not valid UTF-8"))?.into()
                }),
                version: Some(proto::SemanticVersionNumber { major: v.major, minor: v.minor, patch: v.patch }),
            };
            conn.server_started(req).await?;
            Ok(())
        }
        let (rt, mut conn) = self.tokio_connect()?;
        rt.block_on(call_it(&mut conn, data_dir, server_url, db_file))
    }

    /// Make a Tokio runtime and connect to the Organizer
    fn tokio_connect(&self) -> anyhow::Result<(tokio::runtime::Runtime, OrganizerConnection)> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let client = rt.block_on(connect(self.uri.clone()))?;
        Ok((rt, client))
    }
    

}
