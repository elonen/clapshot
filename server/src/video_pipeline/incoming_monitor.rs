#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::os;
use std::time::Duration;
use std::path::{Path, PathBuf};
use file_owner::PathExt;
use async_std::net::Incoming;
use crossbeam_channel::{Sender, Receiver, RecvTimeoutError};
use path_absolutize::*;
use tracing;
use anyhow::anyhow;

use crate::video_pipeline::metadata_reader;
use super::cleanup_rejected::clean_up_rejected_file;

pub enum Void {}

pub fn run_forever(
    data_dir: PathBuf,
    incoming_dir: PathBuf,
    poll_interval: f32,
    resubmit_delay: f32,
    incoming_sender: Sender<super::IncomingFile>,
    exit_evt: Receiver<Void>) -> anyhow::Result<()>
{
    let _span = tracing::info_span!("INCOMING").entered();
    tracing::debug!(dir=data_dir.to_str(), poll_interval=poll_interval, resubmit_delay=resubmit_delay, "Starting.");

    let mut last_tested_size: std::collections::HashMap<PathBuf, u64> = std::collections::HashMap::new();
    let mut submission_time: std::collections::HashMap<PathBuf, std::time::Instant> = std::collections::HashMap::new();

    loop {
        // Remove expired submissions
        let now = std::time::Instant::now();
        submission_time.retain(|_, t| now.duration_since(t.clone()).as_secs_f32() < resubmit_delay);

        match exit_evt.recv_timeout(Duration::from_secs_f32(poll_interval)) {
            Err(RecvTimeoutError::Disconnected) => { break; }
            _ => {}
        }
        //tracing::trace!("Polling dir.");
        match incoming_dir.read_dir() {
            Ok(entries) => {

                let names_and_sizes = entries
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        let stat = entry.metadata().ok()?;
                        stat.is_file().then(|| (entry.path(), stat.len()))
                    }).collect::<Vec<_>>();

                fn get_file_owner_name(path: &Path) -> anyhow::Result<String> {
                    path.owner()?.name()?.ok_or(anyhow!("Unnamed OS user for file {:?}", path))
                }

                for (path, sz) in names_and_sizes {
                    let _span = tracing::debug_span!("Considering file.", path=path.to_str()).entered();

                    if !submission_time.contains_key(&path) {
                        // Check if file is still being written to
                        if sz > 1 && sz != 4096 {  // 4096 = size of an empty file on ext4
                            if &sz == last_tested_size.get(&path).unwrap_or(&0) {
                                match get_file_owner_name(&path) {
                                    Err(e) => {
                                        tracing::error!(details=%e, "Cannot ingest. Failed to get owner's name for file.");
                                        clean_up_rejected_file(&data_dir, &path, None).unwrap_or_else(|e| {
                                            tracing::error!(details=%e, "Clean up also failed.");
                                        });
                                        continue;
                                    }
                                    Ok(owner) => {
                                        tracing::info!("Submitting for processing.");
                                        submission_time.insert(path.clone(), std::time::Instant::now());
                                        if let Err(e) = incoming_sender.send(
                                                super::IncomingFile {file_path: path.clone(), user_id: owner, cookies: HashMap::new()}) {
                                            tracing::error!(details=%e, "Failed to send incoming file to processing queue.");
                                        }
                                    },
                                };
                            } else {
                                tracing::debug!("File '{:?}' apparently still being written to. Skipping for now...", path);
                                last_tested_size.insert(path, sz);
                            }}}}
            },
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
