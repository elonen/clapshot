#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use anyhow::anyhow;
use async_std::net::Incoming;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use file_owner::PathExt;
use path_absolutize::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::os;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing;

use super::{cleanup_rejected::clean_up_rejected_file, IngestUsernameFrom};
use crate::video_pipeline::metadata_reader;

pub enum Void {}

pub fn run_forever(
    data_dir: PathBuf,
    incoming_dir: PathBuf,
    poll_interval: f32,
    resubmit_delay: f32,
    incoming_sender: Sender<super::IncomingFile>,
    exit_evt: Receiver<Void>,
    ingest_username_from: IngestUsernameFrom,
) -> anyhow::Result<()> {
    let _span = tracing::info_span!("INCOMING").entered();
    tracing::debug!(
        dir = data_dir.to_str(),
        poll_interval = poll_interval,
        resubmit_delay = resubmit_delay,
        "Starting."
    );

    let mut last_tested_size: std::collections::HashMap<PathBuf, u64> =
        std::collections::HashMap::new();
    let mut submission_time: std::collections::HashMap<PathBuf, std::time::Instant> =
        std::collections::HashMap::new();

    loop {
        // Remove expired submissions
        let now = std::time::Instant::now();
        submission_time.retain(|_, t| now.duration_since(t.clone()).as_secs_f32() < resubmit_delay);

        match exit_evt.recv_timeout(Duration::from_secs_f32(poll_interval)) {
            Err(RecvTimeoutError::Disconnected) => {
                break;
            }
            _ => {}
        }
        //tracing::trace!("Polling dir.");
        match incoming_dir.read_dir() {
            Ok(entries) => {
                // Collect files from incoming directory and one level of subdirectories
                let mut names_and_sizes = Vec::new();
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() {
                                // File directly in incoming/
                                names_and_sizes.push((path, metadata.len()));
                            } else if metadata.is_dir() {
                                // Look one level deeper in subdirectories
                                if let Ok(subdir_entries) = std::fs::read_dir(&path) {
                                    for subentry in subdir_entries {
                                        if let Ok(subentry) = subentry {
                                            if let Ok(sub_metadata) = subentry.metadata() {
                                                if sub_metadata.is_file() {
                                                    names_and_sizes.push((
                                                        subentry.path(),
                                                        sub_metadata.len(),
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                fn get_file_owner_name(path: &Path) -> anyhow::Result<String> {
                    path.owner()?
                        .name()?
                        .ok_or(anyhow!("Unnamed OS user for file {:?}", path))
                }

                fn get_username_from_folder(
                    path: &Path,
                    incoming_dir: &Path,
                ) -> anyhow::Result<String> {
                    let relative_path = path.strip_prefix(incoming_dir).map_err(|e| {
                        anyhow!(
                            "File {:?} is not within incoming directory {:?}: {}",
                            path,
                            incoming_dir,
                            e
                        )
                    })?;

                    let first_component = relative_path.components().next().ok_or(anyhow!(
                        "File {:?} has no parent directory components",
                        relative_path
                    ))?;

                    match first_component {
                        std::path::Component::Normal(username) => username
                            .to_str()
                            .ok_or(anyhow!(
                                "Username directory name is not valid UTF-8: {:?}",
                                username
                            ))
                            .map(|s| s.to_string()),
                        _ => Err(anyhow!(
                            "Invalid directory structure for file {:?}",
                            relative_path
                        )),
                    }
                }

                for (path, sz) in names_and_sizes {
                    let _span =
                        tracing::debug_span!("Considering file.", path = path.to_str()).entered();

                    if !submission_time.contains_key(&path) {
                        // Check if file is still being written to
                        if sz > 1 && sz != 4096 {
                            // 4096 = size of an empty file on ext4
                            if &sz == last_tested_size.get(&path).unwrap_or(&0) {
                                let username_result = match ingest_username_from {
                                    IngestUsernameFrom::FileOwner => get_file_owner_name(&path),
                                    IngestUsernameFrom::FolderName => {
                                        get_username_from_folder(&path, &incoming_dir)
                                    }
                                };

                                match username_result {
                                    Err(e) => {
                                        tracing::error!(details=%e, "Cannot ingest. Failed to get username for file.");
                                        clean_up_rejected_file(&data_dir, &path, None).unwrap_or_else(|e| {
                                            tracing::error!(details=%e, "Clean up also failed.");
                                        });
                                        continue;
                                    }
                                    Ok(username) => {
                                        tracing::info!("Submitting for processing.");
                                        submission_time
                                            .insert(path.clone(), std::time::Instant::now());
                                        if let Err(e) = incoming_sender.send(super::IncomingFile {
                                            file_path: path.clone(),
                                            user_id: username,
                                            cookies: HashMap::new(),
                                            transcode_preference: super::TranscodePreference::Auto,
                                        }) {
                                            tracing::error!(details=%e, "Failed to send incoming file to processing queue.");
                                        }
                                    }
                                };
                            } else {
                                tracing::debug!("File '{:?}' apparently still being written to. Skipping for now...", path);
                                last_tested_size.insert(path, sz);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Directory listing failed. Cannot continue monitoring.
                tracing::error!(details=%e, "Error monitoring dir {:?} - aborting.",
                match incoming_dir.absolutize() {
                    Ok(Cow::Owned(p)) => p,     // Got absolute path
                    _ => incoming_dir.clone(),  // Some error happened, use original
                });
                break;
            }
        }
    }

    tracing::debug!("Exiting.");
    Ok(())
}
