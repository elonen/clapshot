use std::path::Path;
use anyhow::{anyhow, bail};
use tracing;


/// Clean up after a processing error. Attempts to preserve the original file
/// by moving it under the rejected directory. Then deletes any dangling files that were
/// created during the failed ingestion.
pub fn clean_up_rejected_file(data_dir: &Path, src_file: &Path, media_file_id: Option<String>) -> anyhow::Result<()>
{
    // Create rejected directory if it doesn't exist
    let rejected_dir = data_dir.join("rejected");
    if !rejected_dir.exists() { std::fs::create_dir(&rejected_dir)?; };

    let src_file_name = src_file.file_name().ok_or(anyhow!("Invalid filename {:?}", src_file))?;
    let move_to = rejected_dir.join(src_file_name);
    if !move_to.exists() {
        // Move the original file to the root of rejected directory
        std::fs::rename(src_file, &move_to)?;
    } else {
        // If the destination file already exists, make a subdirectory for the new one.
        // Use media file id if available, otherwise an UUID4.
        let extra_dir = match &media_file_id {
            Some(id) => rejected_dir.join(id),
            None => rejected_dir.join( uuid::Uuid::new_v4().to_string() ),
        };
        if !extra_dir.exists() { std::fs::create_dir(&extra_dir)?; };

        let move_to = extra_dir.join(src_file_name);
        if !move_to.exists() {
            // Move it to the new subdirectory
            std::fs::rename(src_file, move_to)?;
        } else {
            // The file already exists in the new subdirectory. Since id (hash) is equal, it's
            // probably the same file, but check the size to be sure.
            let src_size = std::fs::metadata(src_file)?.len();
            let dest_size = std::fs::metadata(&move_to)?.len();
            if src_size == dest_size {
                tracing::warn!("File '{}' already exists in rejects dir, but size is identical. Deleting original.", move_to.display());
                std::fs::remove_file(src_file)?;
            } else {
                bail!("File '{}' already exists in rejects dir, and size is different. Not deleting original ('{}').", move_to.display(), &src_file.display());
            }
        }
    }

    Ok(())
}
