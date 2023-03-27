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

use anyhow::{anyhow, Context, bail};
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

pub const THUMB_SHEET_COLS: u32 = 10;
pub const THUMB_SHEET_ROWS: u32 = 10;
pub const THUMB_W: u32 = 160;
pub const THUMB_H: u32 = 90;


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
fn calc_video_hash(file_path: &PathBuf, user_id: &str) -> anyhow::Result<String> {
    let mut file_hash = Sha256::new();
    let fname = file_path.file_name()
        .ok_or(anyhow!("Bad filename: {:?}", file_path))?.to_str()
        .ok_or(anyhow!("Bad filename encoding {:?}", file_path))?;
    
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
            -> anyhow::Result<bool>
{
    let _span = tracing::info_span!("INGEST_VIDEO",
        vh = %vh,
        user=md.user_id,
        filename=%md.src_file.file_name().unwrap_or_default().to_string_lossy()).entered();

    tracing::info!(file=%md.src_file.display(), "Ingesting file.");

    let src = PathBuf::from(&md.src_file);
    if !src.is_file() { bail!("Source file not found: {:?}", src) }

    let dir_for_video = videos_dir.join(&vh);
    tracing::debug!("Video dir = {:?}", dir_for_video);

    // Video already exists on disk?
    if dir_for_video.exists() {
        tracing::debug!("Video dir already exists.");
        match db.get_video(&vh) {
            Ok(v) => {
                let new_owner = &md.user_id;
                if v.added_by_userid == Some(new_owner.clone()) {

                    tracing::info!("User already has this video.");
                    user_msg_tx.send(UserMessage {
                        topic: UserMessageTopic::Ok(),
                        msg: "You already have this video".to_string(),
                        details: None,
                        user_id: Some(new_owner.clone()),
                        video_hash: None  // Don't pass video hash here, otherwise the pre-existing video would be deleted!
                    }).ok();

                    clean_up_rejected_file(&data_dir, &src, Some(vh.into())).unwrap_or_else(|e| {
                        tracing::error!(details=?e, "Cleanup failed.");
                    });

                    return Ok(false);
                } else {
                    bail!("Hash collision?!? Video '{vh}' already owned by another user '{new_owner}'.")
                }
            },
            Err(DBError::NotFound()) => {
                // File exists, but not in DB. Remove files and reprocess.
                tracing::info!("Dir for '{vh}' exists, but not in DB. Deleting old dir and reprocessing.");
                std::fs::remove_dir_all(&dir_for_video)?;
            }
            Err(e) => {
                bail!("Error checking DB for video '{}': {}", vh, e);
            }
        }
    }
    assert!(!dir_for_video.exists()); // Should have been deleted above

    // Move src file to orig/
    tracing::debug!(dir=%dir_for_video.display(), "Creating video hash dir.");
    std::fs::create_dir(&dir_for_video)?;

    let dir_for_orig = dir_for_video.join("orig");
    std::fs::create_dir(&dir_for_orig)?;
    let src_moved = dir_for_orig.join(src.file_name().ok_or(anyhow!("Bad filename: {:?}", src))?);

    tracing::debug!("Moving '{}' to '{}'", src.display(), src_moved.display());
    std::fs::rename(&src, &src_moved)?;
    if !src_moved.exists() { bail!("Failed to move {:?} file to orig/", src_moved) }

    let orig_filename = src.file_name().ok_or(anyhow!("Bad filename: {:?}", src))?.to_string_lossy().into_owned();

    // Add to DB
    tracing::info!("Adding video to DB.");
    db.add_video(&models::VideoInsert {
        video_hash: vh.to_string(),
        added_by_userid: Some(md.user_id.clone()),
        added_by_username: Some(md.user_id.clone()),  // TODO: get username from somewhere
        recompression_done: None,
        thumb_sheet_dims: None,
        orig_filename: Some(orig_filename.clone()),
        title: Some(orig_filename),
        total_frames: Some(md.total_frames as i32),
        duration: md.duration.to_f32(),
        fps: Some(md.fps.to_string()),
        raw_metadata_all: Some(md.metadata_all.clone()),
    })?;

    // Check if it needs recompressing
    fn needs_transcoding(md: &metadata_reader::Metadata, target_max_bitrate: u32) -> Option<(String, u32)> {
        let new_bitrate = std::cmp::max(md.bitrate/2, std::cmp::min(md.bitrate, target_max_bitrate));
        let ext = md.src_file.extension().unwrap_or(std::ffi::OsStr::new("")).to_string_lossy().to_lowercase();
        {
            let bitrate_fine = (new_bitrate >= md.bitrate || (md.bitrate as f32) <= 1.2 * (target_max_bitrate as f32));
            let codec_fine = ["h264", "avc", "hevc", "h265"].contains(&md.orig_codec.to_lowercase().as_str());
            let container_fine = ["mp4", "mkv"].contains(&ext.as_str());        
    
            if !container_fine { Some(format!("container '{}' not supported", md.src_file.extension().unwrap_or_default().to_string_lossy())) }
            else if !codec_fine { Some(format!("codec '{}' not supported", md.orig_codec)) }
            else if !bitrate_fine { Some(format!("bitrate is too high: old {} > new {}", md.bitrate, new_bitrate)) }
            else { None }
        }.map(|reason| (reason, new_bitrate) )
    }

    let transcode_req = match needs_transcoding(md, target_bitrate) {
        Some((reason, new_bitrate)) => {
            let video_dst = dir_for_video.join(format!("transcoded_br{}_{}.mp4", new_bitrate, uuid::Uuid::new_v4()));
            cmpr_tx.send(video_compressor::CmprInput {
                src: src_moved.clone(),
                video_dst: Some(video_dst),
                thumb_dir: None,
                video_bitrate: new_bitrate,
                video_hash: vh.to_string(),
                user_id: md.user_id.clone(),
            }).map(|_| (true, reason)).context("Error sending file to transcoding")
        },
        None => {
            tracing::info!("Video ok already, not transcoding.");
            Ok((false, "".to_string()))
        }
    };

    // Also create thumbnails unless there was a problem with the file
    if let Ok(_) = &transcode_req {
        let thumbs_dir = dir_for_video.join("thumbs");
        if let Err(e) = cmpr_tx.send(video_compressor::CmprInput {
                src: src_moved,
                video_dst: None,
                thumb_dir: Some(thumbs_dir),
                video_bitrate: 0,
                video_hash: vh.to_string(),
                user_id: md.user_id.clone(),
            }) {
                tracing::error!(details=?e, "Failed to send file to thumbnailing");
                if let Err(e) = user_msg_tx.send(UserMessage {
                        topic: UserMessageTopic::Error(),
                        msg: "Thumbnailing failed.".to_string(),
                        details: Some(format!("Error sending file to thumbnailing: {}", e)),
                        user_id: Some(md.user_id.clone()),
                        video_hash: Some(vh.to_string())
                    }) { tracing::error!(details=?e, "Failed to send user message") };
        };
    };

    // Format results to user readable message
    match transcode_req {
        Ok((do_transcode, reason)) => {
            tracing::info!(transcode=do_transcode, reason=reason, "Video added to DB. Transcode");
            user_msg_tx.send(UserMessage {
                topic: UserMessageTopic::Ok(),
                msg: "Video added".to_string() + if do_transcode {". Transcoding..."} else {""},
                details: if do_transcode { Some(format!("Transcoding because {reason}")) } else { None },
                user_id: Some(md.user_id.clone()),
                video_hash: Some(vh.to_string())
            })?;
            Ok(do_transcode)
        },
        Err(e) => {
            tracing::error!(details=?e, "Video added to DB, but failed to send to transcoding.");
            user_msg_tx.send(UserMessage {
                topic: UserMessageTopic::Error(),
                msg: "Video added but not transcoded. Video may not play.".to_string(),
                details: Some(format!("Error sending video to transcoder: {}", e)),
                user_id: Some(md.user_id.clone()),
                video_hash: Some(vh.to_string())
            })?;
            Err(e)
        }
    }
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
    tracing::info!("Starting video processing pipeline.");

    // Create folder for processed videos
    let videos_dir = data_dir.join("videos");
    if let Err(e) = std::fs::create_dir_all(&videos_dir) {
        tracing::error!(details=%e, "Error creating videos dir '{}'.", videos_dir.display());
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
                    tracing::error!(details=?e, "Error from incoming monitor.");
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

    // Migration from older version: find a video that is missing thumbnail sheet
    fn legacy_thumbnail_next_video(db: &DB, videos_dir: &PathBuf, cmpr_in: &mut crossbeam_channel::Sender<video_compressor::CmprInput>) -> Option<String> {
        let next = match db.get_all_videos_without_thumbnails() {
            Ok(videos) => videos.first().cloned(),
            Err(e) => {
                tracing::error!(details=?e, "DB: Failed to get videos without thumbnails.");
                return None;
            }};

        if let Some(v) = next {
            tracing::info!(video_hash=%v.video_hash, "Found legacy video that needs thumbnailing.");

            let video_file = if v.recompression_done.is_some() {
                    Some(videos_dir.join(&v.video_hash).join("video.mp4"))
                } else {
                    match v.orig_filename {
                        Some(ref orig_filename) => Some(videos_dir.join(&v.video_hash).join("orig").join(orig_filename)),
                        None => {
                            tracing::error!(video_hash=%v.video_hash, "Legacy thumbnailing failed. Original filename missing and not recompressed.");
                            None
                        }}
                };

            match (&v.added_by_userid, video_file) {
                (Some(user_id), Some(video_file)) => {
                    let req = video_compressor::CmprInput {
                        src: video_file,
                        video_dst: None,
                        thumb_dir: Some(videos_dir.join(&v.video_hash).join("thumbs")),
                        video_bitrate: 0,
                        video_hash: v.video_hash.clone(),
                        user_id: user_id.clone(),
                    };
                    cmpr_in.send(req).unwrap_or_else(|e| {
                            tracing::error!(details=?e, "Error sending legacy thumbnailing request to compressor.");
                        });
                    return Some(v.video_hash.clone());
                },
                _ => {
                    tracing::error!(video_hash=%v.video_hash, "Legacy thumbnailing failed. User ID or orig filename missing.");
                },
            }
        }
        None
    }
    let mut legacy_video_now_thumnailing = legacy_thumbnail_next_video(&db, &videos_dir, &mut cmpr_in_tx.clone());


    let _span = tracing::info_span!("PIPELINE").entered();
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
                                    details: Some(format!("'{}': ", e.src_file.file_name().unwrap_or_default().to_string_lossy()) + &e.details + &cleanup_err),
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

                            // Thumbnails done?
                            if let Some(thumb_dir) = res.thumb_dir {
                                if let Err(e) = db.set_video_thumb_sheet_dimensions(&vh, THUMB_SHEET_COLS, THUMB_SHEET_ROWS) {
                                    tracing::error!(details=%e, "Error storing thumbnail sheet dims in DB");
                                } else {
                                    // Thumbnailer for old videos: find next video to thumbnail, if any
                                    if Some(vh.clone()) == legacy_video_now_thumnailing {
                                        legacy_video_now_thumnailing = legacy_thumbnail_next_video(&db, &videos_dir, &mut cmpr_in_tx.clone());
                                    }
                                }
                                // Write out stdout/stderr to separate files
                                for (name, data) in [("stdout", &res.stdout), ("stderr", &res.stderr)].iter() {
                                    let path = thumb_dir.join(format!("{}.txt", name));
                                    tracing::debug!(video=%vh, file=?path, "Writing {} from thumbnailer", name);
                                    match std::fs::write(&path, data) {
                                        Ok(_) => {},
                                        Err(e) => {
                                            tracing::error!(file=?path, details=%e, "Error writing {:?}", name);
                                }}}

                                // Send VideoUpdated message to user
                                user_msg_tx.send(UserMessage {
                                        topic: UserMessageTopic::VideoUpdated(),
                                        msg: "Video thumbnail generated".into(),
                                        details: None,
                                        user_id: Some(res.user_id),
                                        video_hash: Some(vh.clone())
                                    }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                            }

                            // Video done?
                            if let Some(video_dst) = res.video_dst.clone() {
                                // Write out stdout/stderr to separate files
                                for (name, data) in [("stdout", &res.stdout), ("stderr", &res.stderr)].iter() {
                                    let path = videos_dir.join(&vh).join(format!("{}.txt", name));
                                    tracing::debug!(video=%vh, file=?path, "Writing {} from ffmpeg", name);
                                    match std::fs::write(&path, data) {
                                        Ok(_) => {},
                                        Err(e) => {
                                            tracing::error!(file=?path, details=%e, "Error writing {:?}", name);
                                }}}

                                // Get filename from path
                                fn get_filename(p: &PathBuf) -> anyhow::Result<String> {
                                    Ok(p.file_name().ok_or(anyhow!("bad filename: {}", p.to_string_lossy()))?.to_str().ok_or(anyhow!("bad encoding"))?.to_string())
                                }

                                // Symlink to transcoded file
                                let linked_ok = (move || {
                                    let vh_dir = videos_dir.join(&vh);
                                    if !vh_dir.exists() {
                                        if let Err(e) = std::fs::create_dir(&vh_dir) {
                                            tracing::error!(details=%e, "Video hash dir {:?} was missing after transcoding, and creating it failed. Probably a bug.", vh_dir);
                                            return false;
                                        }}
                                    let dst_filename = match get_filename(&video_dst) {
                                        Ok(f) => f,
                                        Err(e) => { tracing::error!("{:?}", e); return false; }
                                    };
                                    let symlink_path = vh_dir.join("video.mp4");
                                    if let Err(e) = std::os::unix::fs::symlink(dst_filename, &symlink_path) {
                                        tracing::error!(details=%e, "Failed to create symlink {:?} -> {:?}", symlink_path, res.video_dst);
                                        return false;
                                    }
                                    if let Err(e) = db.set_video_recompressed(&vh) {
                                        tracing::error!(details=%e, "Error marking video as recompressed in DB");
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
                                    }).unwrap_or_else(|e| { tracing::error!(details=%e, "Error sending user message"); });
                            }
                        }
                        else {
                            let msg = format!("Video {} failed", if res.video_dst.is_some() {"transcoding"} else {"thumbnailing"});
                            tracing::error!(video=res.video_hash, details=?res.dmsg, msg);
                            user_msg_tx.send(UserMessage {
                                    topic: UserMessageTopic::Error(),
                                    msg: msg,
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

    tracing::debug!("Exiting.");
}
