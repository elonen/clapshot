use std::path::Path;
use anyhow::{bail, Context};
use tracing::debug;

pub (crate) fn delete_old<T>(path: T) -> Result<(), anyhow::Error>
    where T: AsRef<Path> + std::fmt::Debug
{
    let p = path.as_ref();
    Ok(if p.exists() {
        use std::os::unix::fs::FileTypeExt;
        if p.metadata().context("Getting socket metadata")?.file_type().is_socket() {
            debug!("Deleting old socket file before spawning organizer");
            std::fs::remove_file(p).context("Deleting organizer socket file")?;
        } else {
            bail!("Organizer socket path exists but it's not a socket. Refusing to delete.");
        }
    })
}

pub (crate) async fn wait_for(path: &Path, max_secs: f32) -> anyhow::Result<()> {
    let mut secs = max_secs;
    while !path.exists() {
        if secs == max_secs {
            debug!("Waiting for Unix socket to appear: {:?}", path);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        secs -= 0.1;
        if secs <= 0.0 { bail!("Timeout waiting for socket to appear: {:?}", path); }
    }
    if secs < max_secs {
        // Wait a bit more to make sure the socket is ready
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}
