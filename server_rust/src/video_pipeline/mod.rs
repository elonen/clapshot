//#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

#![allow(unused_parens)]

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
mod video_compressor;

use metadata_reader::MetadataResult;
use crate::api_server::{UserMessage, UserMessageTopic};
use crate::database::error::DBError;
use cleanup_rejected::clean_up_rejected_file;
use crate::database::{DB, models};

#[derive (Clone, Debug)]
pub struct IncomingFile {
    pub file_path: PathBuf,
    pub user_id: String,
}

#[derive(Debug, Clone)]
pub struct DetailedMsg {
    pub msg: String,
    pub details: String,
    pub src_file: PathBuf,
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

/// Process new video after metadata reader has finished.
/// Move the file to the appropriate directory, and update the database.
/// See if the video is a duplicate, and submit it for transcoding if necessary.
fn ingest_video(
        vh: &str,
        md: &metadata_reader::Metadata,
        data_dir: &Path,
        videos_dir: &Path,
        target_bitrate: u32,
        db: &DB,
        user_msg_tx: &crossbeam_channel::Sender<UserMessage>,
        cmpr_tx: &crossbeam_channel::Sender<video_compressor::CmprInput>)
            -> Result<bool, Box<dyn std::error::Error>>
{
    tracing::debug!("Video hash for {:?} = {}", md.src_file, vh);

    let src = PathBuf::from(&md.src_file);
    if !src.is_file() { return Err("Source file not found".into()); }

    let dir_for_video = videos_dir.join(&vh);

    // Video already exists on disk?
    if dir_for_video.exists() {
        match db.get_video(&vh) {
            Ok(v) => {
                let new_owner = &md.user_id;
                if v.added_by_userid == Some(new_owner.clone()) {

                    tracing::info!("User '{new_owner}' already has video {vh}");
                    user_msg_tx.send(UserMessage {
                        topic: UserMessageTopic::Ok(),
                        msg: "You already have this video".to_string(),
                        details: None,
                        user_id: Some(new_owner.clone()),
                        video_hash: None  // Don't pass video hash here, otherwise the pre-existing video would be deleted!
                    }).ok();

                    clean_up_rejected_file(&data_dir, &src, Some(vh.into())).unwrap_or_else(|e| {
                        tracing::error!("Cleanup of '{:?}' failed: {:?}", &src, e);
                    });

                    return Ok(false);
                } else {
                    return Err(format!("Hash collision?!? Video '{vh}' already owned by '{new_owner}'.").into());
                }
            },
            Err(DBError::NotFound()) => {
                // File exists, but not in DB. Remove file and reprocess.
                tracing::info!("Dir for '{vh}' exists, but not in DB. Deleting old dir and reprocessing.");
                std::fs::remove_dir_all(&dir_for_video)?;
            }
            Err(e) => {
                return Err(format!("Error checking DB for video '{}': {:?}", vh, e).into());
            }
        }
    }
    assert!(!dir_for_video.exists()); // Should have been deleted above

    // Move src file to orig/
    tracing::debug!("Creating dir '{:?}'...", dir_for_video);
    std::fs::create_dir(&dir_for_video)?;

    let dir_for_orig = dir_for_video.join("orig");
    std::fs::create_dir(&dir_for_orig)?;
    let src_moved = dir_for_orig.join(src.file_name().ok_or("Bad filename")?);

    tracing::debug!("Moving '{:?}' to '{:?}'...", src, src_moved);
    std::fs::rename(&src, &src_moved)?;
    if !src_moved.exists() { return Err("Failed to move src file to orig/".into()); }

    // Add to DB
    tracing::info!("Adding video '{}' to DB. Src '{:?}', owner '{}'", vh, src_moved, md.user_id);
    db.add_video(&models::VideoInsert {
        video_hash: vh.to_string(),
        added_by_userid: Some(md.user_id.clone()),
        added_by_username: Some(md.user_id.clone()),  // TODO: get username from somewhere
        recompression_done: None,
        orig_filename: Some(src.file_name().ok_or("Bad filename")?.to_string_lossy().into_owned()),
        total_frames: Some(md.total_frames as i32),
        duration: md.duration.to_f32(),
        fps: Some(md.fps.to_string()),
        raw_metadata_all: Some(md.metadata_all.clone()),
    })?;

    // Check if it needs recompressing
    fn calc_transcoding_bitrate(md: &metadata_reader::Metadata, target_max_bitrate: u32) -> Option<u32> {
        let new_bitrate = std::cmp::max(md.bitrate/2, std::cmp::min(md.bitrate, target_max_bitrate));
        let ext = md.src_file.extension().unwrap_or(std::ffi::OsStr::new("")).to_string_lossy().to_lowercase();

        let bitrate_fine = (new_bitrate >= md.bitrate || (md.bitrate as f32) <= 1.2 * (target_max_bitrate as f32));
        let codec_fine = ["h264", "avc", "hevc", "h265"].contains(&md.orig_codec.to_lowercase().as_str());
        let container_fine = ["mp4", "mkv"].contains(&ext.as_str());        

        if !bitrate_fine { tracing::info!("Transcoding because: bitrate too high old {} > new {}", md.bitrate, new_bitrate); }
        if !codec_fine { tracing::info!("Transcoding because: codec not supported '{}'", md.orig_codec); }
        if !container_fine { tracing::info!("Transcoding because: container not supported '{:?}'", md.src_file.extension()); }

        if bitrate_fine && codec_fine && container_fine { None } else { Some(new_bitrate) }
    }
    let transcode = match calc_transcoding_bitrate(md, target_bitrate) {
        Some(new_bitrate) => {
            tracing::info!("Video {} ('{:?}') requires transcoding (orig bitrate={}, new={})", vh, md.src_file, md.bitrate, new_bitrate);
            let dst = dir_for_video.join(format!("transcoded_br{}_{}.mp4", new_bitrate, uuid::Uuid::new_v4()));
            cmpr_tx.send(video_compressor::CmprInput {
                src: src_moved,
                dst,
                video_bitrate: new_bitrate,
                video_hash: vh.to_string(),
                user_id: md.user_id.clone(),
            }).map(|_| true).map_err(|e| format!("Error sending to transcoding: {:?}", e))
        },
        None => {
            tracing::info!("Video {} ('{:?}') ok, not transcoding.", vh, md.src_file);
            Ok(false)
        }
    };

    // Send ok message to user
    user_msg_tx.send(UserMessage {
            topic: UserMessageTopic::Ok(),
            msg: "Video added".to_string() + if transcode.clone().unwrap_or(true) {". Transcoding..."} else {""},
            details: None,
            user_id: Some(md.user_id.clone()),
            video_hash: Some(vh.to_string())
        })?;

    transcode.map_err(|e| e.into())
}




pub fn run_forever(
    db: Arc<DB>,
    terminate_flag: Arc<AtomicBool>,
    data_dir: PathBuf,
    user_msg_tx: crossbeam_channel::Sender<UserMessage>,
    poll_interval: f32,
    resubmit_delay: f32,
    target_bitrate: u32,
    upload_rx: Receiver<IncomingFile>,
    n_workers: usize)
{
    tracing::info!("Starting video processing pipeline. Polling interval: {}s, resubmit delay: {}s", poll_interval, resubmit_delay);

    // Create folder for processed videos
    let videos_dir = data_dir.join("videos");
    if let Err(e) = std::fs::create_dir_all(&videos_dir) {
        tracing::error!("Error creating videos dir '{:?}': {:?}", videos_dir, e);
        return;
    }

    // Thread for incoming folder scanner
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
                        data_dir.clone(),
                        (data_dir.join("incoming") ).clone(),
                        poll_interval, resubmit_delay,
                        incoming_sender,
                        exit_recvr) {
                    tracing::error!("Error from incoming monitor: {:?}", e);
                }});
        (th, incoming_recvr, exit_sender)
    };

    // Thread for video compressor
    let (cmpr_in_tx, cmpr_in_rx) = unbounded::<video_compressor::CmprInput>();
    let (cmpr_out_tx, cmpr_out_rx) = unbounded::<video_compressor::CmprOutput>();
    let (cmpr_prog_tx, cmpr_prog_rx) = unbounded::<(String, String, String)>();
    thread::spawn(move || {
        video_compressor::run_forever(cmpr_in_rx, cmpr_out_tx, cmpr_prog_tx, n_workers);
    });

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
                                    tracing::error!("Cleanup of '{:?}' failed: {:?}", &msg.file_path, e);
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
                        let (vh, ing_res) = match md_res {
                            MetadataResult::Ok(md) => {
                                tracing::debug!("Got metadata for {:?}", md.src_file);
                                match calc_video_hash(&md.src_file, &md.user_id) {
                                    Err(e) => {
                                        (None, Err(DetailedMsg {
                                            msg: "Video hashing error".into(),
                                            details: e.to_string(),
                                            src_file: md.src_file.clone(),
                                            user_id: md.user_id.clone(),
                                        }))
                                    },
                                    Ok(vh) => {
                                        let ing_res = ingest_video(&vh, &md, &data_dir, &videos_dir, target_bitrate, &db, &user_msg_tx, &cmpr_in_tx).map_err(|e| {
                                            DetailedMsg {
                                                msg: "Video ingestion failed".into(),
                                                details: e.to_string(),
                                                src_file: md.src_file.clone(),
                                                user_id: md.user_id.clone(),
                                            }});
                                        (Some(vh), ing_res)
                                    },
                                }
                            }
                            MetadataResult::Err(e) => (None, Err(e))
                        };
                        // Relay errors, if any.
                        // No need to send ok message here, variations of it are sent from ingest_video().
                        if let Err(e) = ing_res {
                            tracing::error!("Error ingesting file '{:?}' (owner '{:?}', hash '{:?}'): {:?}", e.src_file, e.user_id, vh, e.msg);
                            let cleanup_err = match clean_up_rejected_file(&data_dir, &e.src_file, None) {
                                    Err(e) => { format!(" Cleanup also failed: {:?}", e) },
                                    Ok(()) => { "".into() } };
                            user_msg_tx.send(UserMessage {
                                    topic: UserMessageTopic::Error(),
                                    msg: "Error reading video metadata.".into(),
                                    details: Some(e.details + &cleanup_err),
                                    user_id: Some(e.user_id),
                                    video_hash: vh
                                }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                        }
                    },
                    Err(e) => { tracing::warn!("Metadata reader is dead ('{:?}'). Exit.", e); break; },
                }
            },
            // Incoming file from monitor
            recv(from_mon) -> msg => {
                match msg {
                    Ok(new_file) => {
                        // Relay to metadata reader
                        to_md.send(new_file).unwrap_or_else(|e| {
                            tracing::error!("FATAL. Error sending file to metadata reader: {:?}", e);
                            terminate_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        });
                    },
                    Err(e) => { tracing::warn!("Metadata reader is dead ('{:?}'). Exit.", e); break; },
                }
            },
            // Video compressor progress
            recv(cmpr_prog_rx) -> msg => {
                match msg {
                    Ok((vh, user_id, msg)) => {
                        user_msg_tx.send(UserMessage {
                                topic: UserMessageTopic::Progress(),
                                msg: msg,
                                details: None,
                                user_id: Some(user_id),
                                video_hash: Some(vh)
                            }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                    },
                    Err(e) => { tracing::warn!("Video compressor is dead ('{:?}'). Exit.", e); break; },
                }
            },
            // Video compressor output
            recv(cmpr_out_rx) -> msg => {
                match msg {
                    Err(e) => { tracing::warn!("Video compressor is dead ('{:?}'). Exit.", e); break; },
                    Ok(res) => {
                        if res.success {

                            let videos_dir = videos_dir.clone();
                            let db = db.clone();
                            let vh = res.video_hash.clone();

                            // Get filename from path
                            fn get_filename(p: &str) -> Result<String, Box<dyn std::error::Error>> {
                                Ok(Path::new(p).file_name().ok_or("bad path")?.to_str().ok_or("bad encoding")?.to_string())
                            }
                            
                            let linked_ok = (move || {
                                let vh_dir = videos_dir.join(&vh);
                                if !vh_dir.exists() {
                                    if let Err(e) = std::fs::create_dir(&vh_dir) {
                                        tracing::error!("Video hash dir '{:?}' was missing after transcoding, and creating it failed. Probably a bug. -- {:?}", vh_dir, e);
                                        return false;
                                    }}
                                let dst_filename = match get_filename(&res.dst_file) {
                                    Ok(f) => f,
                                    Err(e) => { tracing::error!("{:?}", e); return false; }
                                };
                                let symlink_path = vh_dir.join("video.mp4");
                                if let Err(e) = std::os::unix::fs::symlink(dst_filename, &symlink_path) {
                                    tracing::error!("Failed to create symlink '{:?}' -> '{:?}': {:?}", symlink_path, res.dst_file, e);
                                    return false;
                                }
                                if let Err(e) = db.set_video_recompressed(&vh) {
                                    tracing::error!("Error setting video as recompressed: {:?}", e);
                                    return false;
                                }
                                true
                            })();

                            // Send success message
                            user_msg_tx.send(UserMessage {
                                    topic: if linked_ok {UserMessageTopic::Ok()} else {UserMessageTopic::Error()},
                                    msg: "Video transcoded.".to_string() + if linked_ok {""} else {" But linking or DB failed."},
                                    details: None,
                                    user_id: Some(res.dmsg.user_id),
                                    video_hash: Some(res.video_hash.clone())
                                }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                        }
                        else {
                            tracing::error!("Video compression failed for '{:?}': {:?}", res.dst_file, res.dmsg);
                            user_msg_tx.send(UserMessage {
                                    topic: UserMessageTopic::Error(),
                                    msg: "Video transcoding failed.".into(),
                                    details: Some(res.dmsg.details),
                                    user_id: Some(res.dmsg.user_id),
                                    video_hash: Some(res.video_hash)
                                }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                        }
                    }
                }
            }
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
