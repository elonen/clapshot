#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::borrow::Cow;
use std::{time::Duration};
use std::path::{Path, PathBuf};
use crossbeam_channel::{Sender, Receiver, RecvTimeoutError};
use path_absolutize::*;
use tracing;

use crate::video_pipeline::metadata_reader;

pub enum Void {}

pub fn run_forever(
    incoming_dir: PathBuf,
    poll_interval: f32, _resubmit_delay: f32,
    _outq: Sender<String>, exit_evt: Receiver<Void>)
{
    loop {
        match exit_evt.recv_timeout(Duration::from_secs_f32(poll_interval)) {
            Err(RecvTimeoutError::Disconnected) => { break; }
            _ => {}
        }
        tracing::info!("Monitor tick");
        match incoming_dir.read_dir() {
            Ok(entries) => {
                let names_and_sizes = entries
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        let stat = entry.metadata().ok()?;
                        stat.is_file().then(|| (entry.path(), stat.len()))
                    })
                    .collect::<Vec<_>>();


                for (path, sz) in names_and_sizes {
                    println!("Found file: {:?}, size {:?}", path, sz);
                }
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
}
