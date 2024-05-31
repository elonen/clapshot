use std::{fs::File, path::PathBuf};

use anyhow::Context;
use flate2::{write::GzEncoder, Compression};
use anyhow::bail;


/// Backup the SQLite database to a tar.gz file.
/// This is done before migrations.
pub fn backup_sqlite_database( db_file: std::path::PathBuf ) -> anyhow::Result<Option<PathBuf>> {
    if db_file.exists() {
        // Make a tar.gz backup
        let now = chrono::Local::now();
        let backup_path = db_file.with_extension(format!("backup-{}.tar.gz", now.format("%Y-%m-%dT%H_%M_%S")));
        tracing::info!(file=%db_file.display(), backup=%backup_path.display(), "Backing up database before migration.");

        let backup_file = File::create(&backup_path).context("Error creating DB backup file")?;
        let gzip_writer = GzEncoder::new(backup_file, Compression::fast());
        let mut tar_builder = tar::Builder::new(gzip_writer);

        let db_file_prefix = db_file.to_string_lossy().into_owned();
        let suffices = ["", "-wal", "-shm"];

        for entry in std::fs::read_dir(db_file.parent().unwrap()).context("Error reading DB directory")? {
            let entry = entry.context("Error reading DB directory entry")?;
            let path = entry.path();
            for suffix in &suffices {
                if path.to_string_lossy().eq(&format!("{}{}", db_file_prefix, suffix)) {
                    tar_builder.append_path_with_name(&path, path.file_name().unwrap())
                        .context(format!("Error adding file '{}' to tar archive", path.display()))?;
                }
            }
        }
        tar_builder.finish().context("Error finishing tar archive")?;
        Ok(Some(backup_path))
    } else {
        Ok(None)
    }
}


pub fn restore_sqlite_database( db_file: std::path::PathBuf, backup_path: std::path::PathBuf ) -> anyhow::Result<()> {
    if db_file.exists() {
        let _span = tracing::info_span!("restore_sqlite_database").entered();
        tracing::info!(file=%db_file.display(), backup=%backup_path.display(), "Restoring.");

        let backup_file = File::open(&backup_path).context("Error opening DB backup file")?;
        let gzip_reader = flate2::read::GzDecoder::new(backup_file);
        let mut tar = tar::Archive::new(gzip_reader);

        let db_file_prefix = db_file.file_name().context("DB file has no filename")?.to_string_lossy();
        let suffices = ["", "-wal", "-shm"];
        //tar.unpack(db_file.parent().unwrap()).context("Error unpacking DB backup")?;
        for entry in tar.entries().context("Error reading tar archive")? {
            let mut entry = entry.context("Error reading tar entry")?;
            let path = entry.path().context("Error getting tar entry path")?.to_path_buf();

            let path_str = path.to_string_lossy();
            let acceptable_names: Vec<String> = suffices.iter().map(|suffix| format!("{}{}", db_file_prefix, suffix)).collect();

            if acceptable_names.iter().any(|p| path_str.eq(p)) {
                                let dst_file = db_file.parent().expect("DB file had no parent").join(path.file_name().expect("Tar entry has no filename"));
                tracing::debug!(file=?path_str, "Unpacking file from tar.");
                entry.unpack(dst_file).context("Error unpacking file")?;
            } else {
                tracing::warn!(path=?path, expected=?acceptable_names, "Unexpected file in backup tar! Skipping.");
            }
        }
        Ok(())
    } else {
        bail!("Database file does not exist, cannot restore from backup.");
    }
}
