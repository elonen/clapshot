//#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::path::{PathBuf, Path};

use crossbeam_channel;
use crossbeam_channel::{Receiver, unbounded, select};
use rust_decimal::prelude::ToPrimitive;
use tracing;

use sha2::{Sha256, Digest};
use hex;

pub mod incoming_monitor;
pub mod metadata_reader;

mod cleanup_rejected;

use metadata_reader::MetadataResult;
use crate::api_server::UserMessage;
use crate::database::error::DBError;
use cleanup_rejected::clean_up_rejected_file;
use crate::database::{DB, models};

#[derive (Clone, Debug)]
pub struct IncomingFile {
    pub file_path: PathBuf,
    pub user_id: String,
}


/// Calculate identifier ("video_hash") for the submitted video,
/// based on filename, user_id, size and sample of the file contents.
fn calc_video_hash(file_path: &PathBuf, user_id: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut file_hash = Sha256::new();
    let fname = file_path.file_name().ok_or("Bad filename")?.to_str().ok_or("Bad filename encoding")?;
    file_hash.update(fname.as_bytes());
    file_hash.update(user_id.as_bytes());
    file_hash.update(&file_path.metadata()?.len().to_be_bytes());
    
    // Read max 32k of contents
    let file = std::fs::File::open(file_path)?;
    let mut buf = Vec::with_capacity(32*1024);
    file.take(32768u64).read_to_end(&mut buf)?;
    file_hash.update(&buf);

    let hash = hex::encode(file_hash.finalize());
    assert!(hash.len() >= 8);
    Ok(hash[0..8].to_string())
}



pub fn run_forever(
    terminate_flag: Arc<AtomicBool>,
    data_dir: PathBuf,
    user_msg_tx: crossbeam_channel::Sender<UserMessage>,
    poll_interval: f32, resubmit_delay: f32,
    upload_rx: Receiver<IncomingFile>)
{
    tracing::info!("Starting video processing pipeline. Polling interval: {}s, resubmit delay: {}s", poll_interval, resubmit_delay);

    // Thread for incoming monitor
    let (_md_thread, from_md, to_md) = {
            let (arg_sender, arg_recvr) = unbounded::<IncomingFile>();
            let (res_sender, res_recvr) = unbounded::<MetadataResult>();

            let th = thread::spawn(move || {
                    metadata_reader::run_forever(arg_recvr, res_sender, 4);
                });
            (th, res_recvr, arg_sender)
        };


    // Thread for metadata reader
    let (mon_thread, from_mon, mon_exit) = {
        let (incoming_sender, incoming_recvr) = unbounded::<IncomingFile>();
        let (exit_sender, exit_recvr) = unbounded::<incoming_monitor::Void>();

        let data_dir = data_dir.clone();
        let th = thread::spawn(move || {
                if let Err(e) = incoming_monitor::run_forever(
                        (data_dir.join("incoming") ).clone(),
                        poll_interval, resubmit_delay,
                        incoming_sender,
                        exit_recvr) {
                    tracing::error!("Error from incoming monitor: {:?}", e);
                }});
        (th, incoming_recvr, exit_sender)
    };


    loop {
        select! {
            // Pass HTTP upload results to metadata reader
            recv(upload_rx) -> msg => {
                match msg {
                    Ok(msg) => {
                        tracing::info!("Got upload result. Submitting it for processing. {:?}", msg);
                        to_md.send(IncomingFile { 
                            file_path: msg.file_path.clone(),
                            user_id: msg.user_id}).unwrap_or_else(|e| {
                                tracing::error!("Error sending file to metadata reader: {:?}", e);
                                clean_up_rejected_file(&data_dir, &msg.file_path, None).unwrap_or_else(|e| {
                                    tracing::error!("Clean up of '{:?}' also failed: {:?}", &msg.file_path, e);
                                });
                            });
                    },
                    Err(_) => { break; }
                }
            }
            // Metadata reader results
            recv(from_md) -> msg => {
                match msg {
                    Ok(md_res) => { 
                        match md_res {
                            MetadataResult::Ok(md) => {
                                tracing::debug!("Got metadata for {:?}", md.src_file);

                                // TODO: pass along to video ingestion

                                fn ingest_video(
                                        md: &metadata_reader::Metadata,
                                        videos_dir: &Path,
                                        db: &DB,
                                        user_msg_tx: &crossbeam_channel::Sender<UserMessage>
                                    ) -> Result<(), Box<dyn std::error::Error>>
                                {
                                    let vh = calc_video_hash(&md.src_file, &md.user_id)?;
                                    tracing::debug!("Video hash for {:?} = {}", md.src_file, vh);

                                    let src = PathBuf::from(&md.src_file);
                                    if !src.is_file() { return Err("Source file not found".into()); }

                                    let new_dir = videos_dir.join(&vh);
                                    if new_dir.exists() {
                                        match db.get_video(&vh) {
                                            Ok(v) => {
                                                let new_owner = &md.user_id;
                                                if v.added_by_userid == Some(new_owner.clone()) {
                                                    tracing::info!("User '{new_owner}' already has video {vh}");
                                                    user_msg_tx.send(UserMessage {
                                                        ok: true,
                                                        msg: format!("Error reading video metadata."),
                                                        details: None,
                                                        user_id: new_owner.clone(),
                                                        video_hash: Some(vh)
                                                    }).ok();
                                                    return Ok(());
                                                } else {
                                                    return Err(format!("Hash collision?!? Video '{vh}' already owned by '{new_owner}'.").into());
                                                }
                                            },
                                            Err(DBError::NotFound()) => {
                                                // File exists, but not in DB. Remove file and reprocess.
                                                tracing::info!("Dir for '{vh}' exists, but not in DB. Deleting old dir and reprocessing.");
                                                std::fs::remove_dir_all(&new_dir)?;
                                            }
                                            Err(e) => {
                                                return Err(format!("Error checking DB for video '{}': {:?}", vh, e).into());
                                            }
                                        }
                                    }
                                    assert!(!new_dir.exists()); // Should have been deleted above

                                    // Move src file to orig/
                                    tracing::debug!("Creating dir '{:?}'...", new_dir);
                                    std::fs::create_dir(&new_dir)?;

                                    let dir_for_orig = new_dir.join("orig");
                                    std::fs::create_dir(&dir_for_orig)?;
                                    let new_src = dir_for_orig.join(src.file_name().ok_or("Bad filename")?);

                                    tracing::debug!("Moving '{:?}' to '{:?}'...", src, new_src);
                                    std::fs::rename(&src, &new_src)?;
                                    if !new_src.exists() { return Err("Failed to move src file to orig/".into()); }

                                    // Add to DB
                                    tracing::info!("Adding video '{}' to DB. Src '{:?}', owner '{}'", vh, new_src, md.user_id);
                                    db.add_video(&models::VideoInsert {
                                        video_hash: vh.clone(),
                                        added_by_userid: Some(md.user_id.clone()),
                                        added_by_username: Some(md.user_id.clone()),  // TODO: get username from somewhere
                                        recompression_done: None,
                                        orig_filename: Some(src.file_name().ok_or("Bad filename")?.to_string_lossy().into_owned()),
                                        total_frames: Some(md.total_frames as i32),
                                        duration: md.duration.to_f32(),
                                        fps: Some(md.fps.to_string()),
                                        raw_metadata_all: Some(md.metadata_all.clone()),
                                    })?;


                                    // TODO: add to recompression queue if necessary
                                    /*
                                        # Schedule recompression if needed
                                        new_bitrate = self._calc_recompression_bitrate(md)
                                        if new_bitrate:
                                            self.compress_q.put( video_compressor.Args(
                                                    src = src,
                                                    dst = new_dir / f'temp_{uuid4()}.mp4',
                                                    video_bitrate = new_bitrate,
                                                    video_hash = video_hash,
                                                    user_id = md.user_id
                                                ))
                                    */

                                    Ok(())
                                }
                                /*
                                // TODO:

                                if let Err(e) = ingest_video(&md, &data_dir.join("videos"), &db,) {
                                        clean_up_rejected_file(&data_dir, &md.src_file, Some(format!("Error: {:?}", e))).unwrap_or_else(|e| {
                                            tracing::error!("Clean up of '{:?}' also failed: {:?}", &md.src_file, e);
                                        });
                                }
                                */
                            }
                            MetadataResult::Err(e) => {
                                let details = format!("File: '{:?}'. Error: {} -- {}", e.src_file.file_name(), e.msg, e.details);
                                tracing::error!("Metadata reader error: {}", details);

                                let cleanup_err = match clean_up_rejected_file(&data_dir, &e.src_file, None) {
                                        Err(e) => { format!(" Cleanup also failed: {:?}", e) },
                                        Ok(()) => { "".into() }
                                    };

                                user_msg_tx.send(UserMessage {
                                        ok: false,
                                        msg: format!("Error reading video metadata."),
                                        details: Some(details + &cleanup_err),
                                        user_id: e.user_id,
                                        video_hash: None
                                    }).ok();
                            }
                        }
                    },
                    Err(e) => { tracing::warn!("Metadata reader is dead ('{:?}'). Exit.", e); break; },
                }
            },
            // Incoming monitor
            recv(from_mon) -> msg => {
                match msg {
                    Ok(new_file) => {
                        tracing::info!("New file in incoming: {:?}", new_file);
                        assert!(false);
                    },
                    Err(e) => { tracing::warn!("Metadata reader is dead ('{:?}'). Exit.", e); break; },
                }
            },
        }

        if mon_thread.is_finished() {
            tracing::error!("Incoming monitor finished. Exit.");
            break;
        }

        if terminate_flag.load(std::sync::atomic::Ordering::Relaxed) {
            tracing::info!("Termination flag set. Exit.");
            break;
        }
    }

    drop(mon_exit);
    terminate_flag.store(true, std::sync::atomic::Ordering::Relaxed);
    match mon_thread.join() {
        Ok(_) => {},
        Err(e) => {
            tracing::error!("Error waiting for monitor thread to exit: {:?}", e);
        }
    }

    tracing::warn!("Clean exit.");
}
