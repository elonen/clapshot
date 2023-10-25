use std::path::{Path, PathBuf};

use lib_clapshot_grpc::{unix_socket, subprocess::spawn_shell, subprocess::ProcHandle};
use lib_clapshot_grpc::proto::org::organizer_inbound_client::OrganizerInboundClient;

use anyhow::{Context, bail};
use tokio::net::UnixStream;
use tonic::transport::{Endpoint, Uri, Channel};
use tower::service_fn;
use tracing::info_span;


pub type OrganizerConnection = OrganizerInboundClient<Channel>;

#[derive(Debug, Clone)]
pub enum OrganizerURI {
    UnixSocket(PathBuf),
    Http(String),
}

/// Connect to a gRPC server, either via a Unix socket or HTTP(S).
/// Plain path string means Unix socket, "http://..." or "https://..." means HTTP(S).
pub async fn connect(uri: OrganizerURI) -> anyhow::Result<OrganizerConnection>
{
    let channel = match uri {
        OrganizerURI::UnixSocket(path) =>
        {
            unix_socket::wait_for(&path, 5.0).await?;
            Endpoint::try_from("file://dummy")?
                .connect_timeout(std::time::Duration::from_secs(8))
                .connect_with_connector(service_fn(move |_: Uri| {
                    UnixStream::connect(path.clone()) })).await
                .context("UnixStream::connect failed")?
        },
        OrganizerURI::Http(uri) =>
        {
            Channel::from_shared(uri.to_string()).context("Failed to parse organizer HTTP URI")?
                .connect_timeout(std::time::Duration::from_secs(8))
                .connect().await.context("HTTP Channel::connect failed")?
        },
    };
    Ok(OrganizerInboundClient::new(channel))
}

/// Parse Organizer plugin arguments and spawn it if necessary
pub fn prepare_organizer(
        org_uri: &Option<String>,
        cmd: &Option<String>,
        debug: bool,
        json: bool,
        data_dir: &Path)
    -> anyhow::Result<(Option<OrganizerURI>, Option<ProcHandle>)>
{
    let mut org_uri = match org_uri {
        None => None,
        Some(s) => Some(match s.split_once("://") {
            Some(("http", _)) | Some(("https", _)) => OrganizerURI::Http(s.clone()),
            Some(("unix", p)) | Some(("file", p)) => OrganizerURI::UnixSocket(p.into()),
            None => OrganizerURI::UnixSocket(s.into()),
            Some((pcol, _)) => bail!("Unsupported gRPC protocol: {}", pcol),
        }),
    };
    let org_hdl =
        if let Some(cmd) = cmd {
            // Use a temp sock if none was given
            if org_uri.is_none() {
                let unix_sock = data_dir
                    .canonicalize().context("Expanding data dir")?
                    .join("grpc-srv-to-org.sock");
                org_uri = Some(OrganizerURI::UnixSocket(unix_sock));
            };
            Some(spawn_organizer(&cmd.as_str(), org_uri.clone().unwrap(), debug, json)?)
        } else { None };

    Ok((org_uri, org_hdl))
}

/// Spawn organizer gRPC server as a subprocess.
/// Dropping the returned handle will signal/kill the subprocess.
fn spawn_organizer(cmd: &str, uri: OrganizerURI, debug: bool, json: bool)
    -> anyhow::Result<ProcHandle>
{
    assert!(cmd != "", "Empty organizer command");

    let mut cmd = match uri {
        OrganizerURI::UnixSocket(path) => {
            unix_socket::delete_old(&path)?;
            format!("{} {}", cmd, path.display())
        },
        OrganizerURI::Http(_) => {
            cmd.into()
        },
    };

    if debug { cmd += " --debug"; }
    if json { cmd += " --json"; }
    spawn_shell(&cmd, "organizer", info_span!("ORG"))
}
