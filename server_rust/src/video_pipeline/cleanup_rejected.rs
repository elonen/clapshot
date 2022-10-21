use std::path::Path;
use tracing;


/// Clean up after a video processing error. Attempts to preserve the original file
/// by moving it under the rejected directory. Then deletes any dangling files that were
/// created during the failed ingestion.
pub fn clean_up_rejected_file(data_dir: &Path, src_file: &Path, video_hash: Option<String>)
        -> Result<(), Box<dyn std::error::Error>>
{
    // Create rejected directory if it doesn't exist
    let rejected_dir = data_dir.join("rejected");
    if !rejected_dir.exists() { std::fs::create_dir(&rejected_dir)?; };

    let move_to = rejected_dir.join(src_file.file_name().ok_or("Invalid filename")? );
    if !move_to.exists() {
        // Move the original file to the root of rejected directory
        std::fs::rename(src_file, &move_to)?;
    } else {
        // If the destination file already exists, make a subdirectory for the new one.
        // Use video hash if available, otherwise an UUID4.
        let extra_dir = match &video_hash {
            Some(hash) => rejected_dir.join(hash),
            None => rejected_dir.join( uuid::Uuid::new_v4().to_string() ),
        };
        if !extra_dir.exists() { std::fs::create_dir(&extra_dir)?; };
        
        let move_to = extra_dir.join(src_file.file_name().ok_or("Invalid filename")? );
        if !move_to.exists() {
            // Move it to the new subdirectory
            std::fs::rename(src_file, move_to)?;
        } else {
            // The file already exists in the new subdirectory. Since hash is equal, it's
            // probably the same file, but check the size to be sure.
            let src_size = std::fs::metadata(src_file)?.len();
            let dest_size = std::fs::metadata(&move_to)?.len();
            if src_size == dest_size {
                tracing::warn!("File '{:?}' already exists in rejects dir, but size is identical. Deleting original.", move_to);
                std::fs::remove_file(src_file)?;
            } else {
                return Err(format!("File '{:?}' already exists in rejects dir, and size is different. Not deleting original ({:?}).", move_to, &src_file).into());
            }
        }
        // Purge (rm -rf) video hash dir if it exists
        if let Some(vh) = video_hash {
            assert!(vh.len() > 0);
            let video_dir = data_dir.join("videos").join(&vh);
            if video_dir.exists() {
                tracing::info!("File '{:?}' was rejected. Deleting video dir '{:?}'.", src_file, video_dir);
                std::fs::remove_dir_all(video_dir)?;
            }
        }
    }

    Ok(())
}
