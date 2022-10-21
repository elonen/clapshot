#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::borrow::Cow;
use std::os;
use std::{time::Duration};
use std::path::{Path, PathBuf};
use file_owner::PathExt;
use async_std::net::Incoming;
use crossbeam_channel::{Sender, Receiver, RecvTimeoutError};
use path_absolutize::*;
use tracing;

use crate::video_pipeline::metadata_reader;

pub enum Void {}


/*
PYTHON VERSION

def monitor_incoming_folder_loop(
    incoming_dir: Path,
    files_to_process: Queue,
    poll_interval: float,
    resubmit_delay: float) -> None:
    """
    Monitor a folder and put new files to processing queue.
    Waits for a file to be fully written (not growing) before putting it to the queue.

    Args:
        incoming_dir:       Path to the incoming/ folder
        files_to_process:   Queue to put new files to
        poll_interval:      How often to check for new files (in seconds)
        resubmit_delay:     How long to wait before resubmitting a file if it was already submitted.
                            This basically specifies how long processing of a file should take at maximum.
    """

    install_sigterm_handlers()

    try:
        logger = logging.getLogger("incoming")

        incoming = Path(incoming_dir)
        assert incoming.is_dir(), f"Path '{incoming}' is not a directory."
        logger.info(f"Starting incoming folder monitor in '{incoming}'...")

        last_tested_size: DefaultDict[Path, int] = DefaultDict(int) # For detecting files that are still being written to
        submission_time: DefaultDict[Path, float] = DefaultDict(float)

        while True:
            logger.debug("Checking for new files...")

            # Remove expired submissions
            submission_time = {k: v for k, v in submission_time.items() if time.time() - v < resubmit_delay}

            # Check for new files in the incoming folder
            for fn in incoming.iterdir():
                if fn.is_file() and not submission_time.get(fn):

                    # Check if file is still being written to
                    cur_size = fn.stat().st_size
                    if cur_size > 1 and cur_size != 4096:  # 4096 is the size of an empty file on ext4
                        if cur_size == last_tested_size[fn]:
                            logger.info(f"Submitting '{fn}' for processing. ")
                            files_to_process.put(str(fn.absolute()))
                            submission_time[fn] = time.time()
                            del last_tested_size[fn]
                        else:
                            logger.info(f"File '{fn}' size changed since last poll. Skipping it for now...")
                            last_tested_size[fn] = cur_size

            # Wait for a bit before checking again
            time.sleep(poll_interval)

    except KeyboardInterrupt:
        pass

    logger.info("Incoming monitor stopped.")
*/


pub fn run_forever(
    incoming_dir: PathBuf,
    poll_interval: f32,
    resubmit_delay: f32,
    incoming_sender: Sender<super::IncomingFile>,
    exit_evt: Receiver<Void>) -> Result<(), Box<dyn std::error::Error>>
{
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
        tracing::debug!("Polling incoming");
        match incoming_dir.read_dir() {
            Ok(entries) => {

                let names_and_sizes = entries
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        let stat = entry.metadata().ok()?;
                        stat.is_file().then(|| (entry.path(), stat.len()))
                    }).collect::<Vec<_>>();

                fn get_file_owner_name(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
                    path.owner()?.name()?.ok_or("Unnamed OS user".into())
                }

                for (path, sz) in names_and_sizes {
                    if !submission_time.contains_key(&path) {
                        // Check if file is still being written to
                        if sz > 1 && sz != 4096 {  // 4096 = size of an empty file on ext4
                            if &sz == last_tested_size.get(&path).unwrap_or(&0) {
                                tracing::info!("Submitting {:?} for processing.", &path);
                                match get_file_owner_name(&path) {
                                    Err(e) => {
                                        tracing::error!("Cannot ingest file. Failed to get owner's name for '{:?}': {}", &path, e);


                                        // TODO: Move file to rejected/ folder


                                        continue;
                                    }
                                    Ok(owner) => {
                                        if let Err(e) = incoming_sender.send(
                                                super::IncomingFile {file_path: path.clone(), user_id: owner}) {
                                            tracing::error!("Failed to send incoming file '{:?}' to processing queue: {:?}", &path, e);
                                        }
                                    },
                                };
                            } else {
                                tracing::info!("File '{:?}' apparently still being written to. Skipping for now...", path);
                                last_tested_size.insert(path, sz);
                            }}}}
            },
            Err(e) => {
                // Directory listing failed. Cannot continue monitoring.
                tracing::error!("Error monitoring {:?} - aborting: {:?}",
                    match incoming_dir.absolutize() {
                        Ok(Cow::Owned(p)) => p,     // Got absolute path
                        _ => incoming_dir.clone(),  // Some error happened, use original
                    },
                    e);
                break;
            }
        }
    }

    tracing::warn!("Clean exit.");
    Ok(())
}
