//#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

#![allow(unused_parens)]

use std::collections::HashMap;
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
use crate::database::{DB, models, DbBasicQuery};

pub const THUMB_SHEET_COLS: u32 = 10;
pub const THUMB_SHEET_ROWS: u32 = 10;
pub const THUMB_W: u32 = 160;
pub const THUMB_H: u32 = 90;


#[derive (Clone, Debug)]
pub struct IncomingFile {
    pub file_path: PathBuf,
    pub user_id: String,
    pub cookies: HashMap<String, String>  // Cookies from client, if this was an HTTP upload
}

#[derive(Debug, Clone)]
pub struct DetailedMsg {
    pub msg: String,
    pub details: String,
    pub src_file: PathBuf,
    pub user_id: String,
}


/// Calculate hash identifier (media_file_id) for the submitted files,
/// based on filename, user_id, size and sample of the file contents.
fn calc_media_file_id(file_path: &PathBuf, user_id: &str, upload_cookies: HashMap<String, String>) -> anyhow::Result<String> {
    let mut file_hash = Sha256::new();
    let fname = file_path.file_name()
        .ok_or(anyhow!("Bad filename: {:?}", file_path))?.to_str()
        .ok_or(anyhow!("Bad filename encoding {:?}", file_path))?;

    file_hash.update(fname.as_bytes());
    file_hash.update(user_id.as_bytes());
    file_hash.update(&file_path.metadata()?.len().to_be_bytes());

    // Add cookies to hash, if any. This allows the same file to be uploaded
    // multiple times with different cookies, e.g. into different folders.
    if !upload_cookies.is_empty() {
        let mut cookies = upload_cookies.iter().collect::<Vec<_>>();
        cookies.sort();
        for (k, v) in cookies {
            file_hash.update(k.as_bytes());
            file_hash.update(v.as_bytes());
        }
    }

    // Read max 32k of contents
    let file = std::fs::File::open(file_path)?;
    let mut buf = Vec::with_capacity(32*1024);
    file.take(32768u64).read_to_end(&mut buf)?;
    file_hash.update(&buf);

    let hash = hex::encode(file_hash.finalize());
    assert!(hash.len() >= 8);
    Ok(hash[0..8].to_string())
}

/// Process new file after metadata reader has finished.
/// Move the file to the appropriate directory, and update the database.
/// See if the file is a duplicate, and submit it for transcoding if necessary.
fn ingest_media_file(
        vid: &str,
        md: &metadata_reader::Metadata,
        data_dir: &Path,
        media_files_dir: &Path,
        target_bitrate: u32,
        db: &DB,
        user_msg_tx: &crossbeam_channel::Sender<UserMessage>,
        cmpr_tx: &crossbeam_channel::Sender<video_compressor::CmprInput>)
            -> anyhow::Result<bool>
{
    let _span = tracing::info_span!("INGEST_MEDIA",
        vid = %vid,
        user=md.user_id,
        filename=%md.src_file.file_name().unwrap_or_default().to_string_lossy()).entered();

    tracing::info!(file=%md.src_file.display(), "Ingesting file.");

    let src = PathBuf::from(&md.src_file);
    if !src.is_file() { bail!("Source file not found: {:?}", src) }

    let dir_for_media_file = media_files_dir.join(&vid);
    tracing::debug!("Media dir = {:?}", dir_for_media_file);

    // File already exists on disk?
    if dir_for_media_file.exists() {
        tracing::debug!("Media dir already exists.");
        match models::MediaFile::get(&mut db.conn()?, &vid.into()) {
            Ok(v) => {
                let new_owner = &md.user_id;
                if &v.user_id == new_owner {
                    tracing::info!("User already has this media file.");
                    user_msg_tx.send(UserMessage {
                        topic: UserMessageTopic::Ok,
                        msg: "Media file already exists".to_string(),
                        details: None,
                        user_id: Some(new_owner.clone()),
                        media_file_id: None  // Don't pass media file id here, otherwise the pre-existing media would be deleted!
                    }).ok();

                    clean_up_rejected_file(&data_dir, &src, Some(vid.into())).unwrap_or_else(|e| {
                        tracing::error!(details=?e, "Cleanup failed.");
                    });

                    return Ok(false);
                } else {
                    bail!("Hash collision?!? Media '{vid}' already owned by another user '{new_owner}'.")
                }
            },
            Err(DBError::NotFound()) => {
                // File exists, but not in DB. Remove files and reprocess.
                tracing::info!("Dir for '{vid}' exists, but not in DB. Deleting old dir and reprocessing.");
                std::fs::remove_dir_all(&dir_for_media_file)?;
            }
            Err(e) => {
                bail!("Error checking DB for media file '{}': {}", vid, e);
            }
        }
    }
    assert!(!dir_for_media_file.exists()); // Should have been deleted above

    // Move src file to orig/
    tracing::debug!(dir=%dir_for_media_file.display(), "Creating media file dir.");
    std::fs::create_dir(&dir_for_media_file)?;

    let dir_for_orig = dir_for_media_file.join("orig");
    std::fs::create_dir(&dir_for_orig)?;
    let src_moved = dir_for_orig.join(src.file_name().ok_or(anyhow!("Bad filename: {:?}", src))?);

    tracing::debug!("Moving '{}' to '{}'", src.display(), src_moved.display());
    std::fs::rename(&src, &src_moved)?;
    if !src_moved.exists() { bail!("Failed to move {:?} file to orig/", src_moved) }

    let orig_filename = src.file_name().ok_or(anyhow!("Bad filename: {:?}", src))?.to_string_lossy().into_owned();

    // Add to DB
    tracing::info!("Adding media file to DB.");
    models::MediaFile::insert(&mut db.conn()?, &models::MediaFileInsert {
        id: vid.to_string(),
        user_id: md.user_id.clone(),
        media_type: Some("video".to_string()),
        recompression_done: None,
        thumb_sheet_cols: None,
        thumb_sheet_rows: None,
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
            let video_dst = dir_for_media_file.join(format!("transcoded_br{}_{}.mp4", new_bitrate, uuid::Uuid::new_v4()));
            cmpr_tx.send(video_compressor::CmprInput {
                src: src_moved.clone(),
                video_dst: Some(video_dst),
                thumb_dir: None,
                video_bitrate: new_bitrate,
                media_file_id: vid.to_string(),
                user_id: md.user_id.clone(),
            }).map(|_| (true, reason)).context("Error sending file to transcoding")
        },
        None => {
            tracing::info!("Media OK already, not transcoding.");
            Ok((false, "".to_string()))
        }
    };

    // Also create thumbnails unless there was a problem with the file
    if let Ok(_) = &transcode_req {
        let thumbs_dir = dir_for_media_file.join("thumbs");
        if let Err(e) = cmpr_tx.send(video_compressor::CmprInput {
                src: src_moved,
                video_dst: None,
                thumb_dir: Some(thumbs_dir),
                video_bitrate: 0,
                media_file_id: vid.to_string(),
                user_id: md.user_id.clone(),
            }) {
                tracing::error!(details=?e, "Failed to send file to thumbnailing");
                if let Err(e) = user_msg_tx.send(UserMessage {
                        topic: UserMessageTopic::Error,
                        msg: "Thumbnailing failed.".to_string(),
                        details: Some(format!("Error sending file to thumbnailing: {}", e)),
                        user_id: Some(md.user_id.clone()),
                        media_file_id: Some(vid.to_string())
                    }) { tracing::error!(details=?e, "Failed to send user message") };
        };
    };

    // Format results to user readable message
    match transcode_req {
        Ok((do_transcode, reason)) => {
            // Notify client about the new media file
            user_msg_tx.send(UserMessage {
                topic: UserMessageTopic::MediaFileAdded,
                msg: String::new(),
                details: Some(serde_json::to_string(&md.upload_cookies).map_err(|e| anyhow!("Error serializing cookies: {}", e))?),
                user_id: Some(md.user_id.clone()),
                media_file_id: Some(vid.to_string())
            })?;
            // Tell user in text also
            tracing::info!(transcode=do_transcode, reason=reason, "Media added to DB. Transcode");
            user_msg_tx.send(UserMessage {
                topic: UserMessageTopic::Ok,
                msg: "Media added.".to_string() + if do_transcode {" Transcoding..."} else {""},
                details: if do_transcode { Some(format!("Transcoding because {reason}")) } else { None },
                user_id: Some(md.user_id.clone()),
                media_file_id: Some(vid.to_string())
            })?;
            Ok(do_transcode)
        },
        Err(e) => {
            tracing::error!(details=?e, "Media added to DB, but failed to send to transcoding.");
            user_msg_tx.send(UserMessage {
                topic: UserMessageTopic::Error,
                msg: "Media added but not transcoded. It may not play.".to_string(),
                details: Some(format!("Error sending file to transcoder: {}", e)),
                user_id: Some(md.user_id.clone()),
                media_file_id: Some(vid.to_string())
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
    tracing::info!("Starting media file processing pipeline.");

    // Create folder for processed media files
    let media_files_dir = data_dir.join("videos");
    if let Err(e) = std::fs::create_dir_all(&media_files_dir) {
        tracing::error!(details=%e, "Error creating media files dir '{}'.", media_files_dir.display());
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

    // Thread for the compressor
    let (cmpr_in_tx, cmpr_in_rx) = unbounded::<video_compressor::CmprInput>();
    let (cmpr_out_tx, cmpr_out_rx) = unbounded::<video_compressor::CmprOutput>();
    let (cmpr_prog_tx, cmpr_prog_rx) = unbounded::<(String, String, String)>();
    thread::spawn(move || {
        video_compressor::run_forever(cmpr_in_rx, cmpr_out_tx, cmpr_prog_tx, n_workers);
    });

    // Migration from older version: find a media file that is missing thumbnail sheet
    fn legacy_thumbnail_next_media_file(db: &DB, videos_dir: &PathBuf, cmpr_in: &mut crossbeam_channel::Sender<video_compressor::CmprInput>) -> Option<String> {

        let next = db.conn()
            .and_then(|mut conn| models::MediaFile::get_all_with_missing_thumbnails(&mut conn))
            .map(|media_files| media_files.first().cloned())
            .map_err(|e| {
                tracing::error!(details=?e, "DB: Failed to get media files without thumbnails or connection.");
            }).ok()?;

        if let Some(v) = next {
            tracing::info!(media_file_id=%v.id, "Found legacy media file that needs thumbnailing.");

            let media_file_path = if v.recompression_done.is_some() {
                    Some(videos_dir.join(&v.id).join("video.mp4"))
                } else {
                    match v.orig_filename {
                        Some(ref orig_filename) => Some(videos_dir.join(&v.id).join("orig").join(orig_filename)),
                        None => {
                            tracing::error!(media_file_id=%v.id, "Legacy thumbnailing failed. Original filename missing and not recompressed.");
                            None
                        }}
                };

            match media_file_path {
                Some(file_path) => {
                    let req = video_compressor::CmprInput {
                        src: file_path,
                        video_dst: None,
                        thumb_dir: Some(videos_dir.join(&v.id).join("thumbs")),
                        video_bitrate: 0,
                        media_file_id: v.id.clone(),
                        user_id: v.user_id.clone(),
                    };
                    cmpr_in.send(req).unwrap_or_else(|e| {
                            tracing::error!(details=?e, "Error sending legacy thumbnailing request to compressor.");
                        });
                    return Some(v.id.clone());
                },
                _ => {
                    tracing::error!(media_file_id=%v.id, "Legacy thumbnailing failed. User ID or orig filename missing.");
                },
            }
        }
        None
    }
    let mut legacy_media_file_now_thumnailing = legacy_thumbnail_next_media_file(&db, &media_files_dir, &mut cmpr_in_tx.clone());


    let _span = tracing::info_span!("PIPELINE").entered();
    loop {
        select! {
            // Pass HTTP upload results to metadata reader
            recv(upload_rx) -> msg => {
                match msg {
                    Ok(msg) => {
                        tracing::info!("Got upload result. Submitting it for processing. {:?}", msg);
                        to_md.send(IncomingFile {file_path: msg.file_path.clone(),user_id: msg.user_id, cookies: msg.cookies }).unwrap_or_else(|e| {
                                tracing::error!("Error sending file to metadata reader: {:?}", e);
                                clean_up_rejected_file(&data_dir, &msg.file_path, None).unwrap_or_else(|e| {
                                    tracing::error!("Cleanup of '{:?}' failed: {:?}", &msg.file_path, e);
                                });
                            },
                        );
                    },
                    Err(_) => { break; }
                }
            }
            // Metadata reader results
            recv(from_md) -> msg => {
                match msg {
                    Ok(md_res) => {
                        let (vid, ing_res) = match md_res {
                            MetadataResult::Ok(md) => {
                                tracing::debug!("Got metadata for {:?}", md.src_file);
                                match calc_media_file_id(&md.src_file, &md.user_id, md.upload_cookies.clone()) {
                                    Err(e) => {
                                        (None, Err(DetailedMsg {
                                            msg: "Media file hashing error".into(),
                                            details: e.to_string(),
                                            src_file: md.src_file.clone(),
                                            user_id: md.user_id.clone(),
                                        }))
                                    },
                                    Ok(vid) => {
                                        let ing_res = ingest_media_file(&vid, &md, &data_dir, &media_files_dir, target_bitrate, &db, &user_msg_tx, &cmpr_in_tx).map_err(|e| {
                                            DetailedMsg {
                                                msg: "Media ingestion failed".into(),
                                                details: e.to_string(),
                                                src_file: md.src_file.clone(),
                                                user_id: md.user_id.clone(),
                                            }});
                                        (Some(vid), ing_res)
                                    },
                                }
                            }
                            MetadataResult::Err(e) => (None, Err(e))
                        };
                        // Relay errors, if any.
                        // No need to send ok message here, variations of it are sent from ingest_media_file().
                        if let Err(e) = ing_res {
                            tracing::error!("Error ingesting file '{:?}' (owner '{:?}', id '{:?}'): {:?}", e.src_file, e.user_id, vid, e.msg);
                            let cleanup_err = match clean_up_rejected_file(&data_dir, &e.src_file, None) {
                                    Err(e) => { format!(" Cleanup also failed: {:?}", e) },
                                    Ok(()) => { "".into() } };
                            user_msg_tx.send(UserMessage {
                                    topic: UserMessageTopic::Error,
                                    msg: "Error reading media file metadata.".into(),
                                    details: Some(format!("'{}': ", e.src_file.file_name().unwrap_or_default().to_string_lossy()) + &e.details + &cleanup_err),
                                    user_id: Some(e.user_id),
                                    media_file_id: vid
                                }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                        }
                    },
                    Err(e) => { tracing::info!("Metadata reader disconnected ('{:?}'). Exit.", e); break; },
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
                    Err(e) => {
                        tracing::info!("Metadata reader disconnected ('{:?}'). Exit.", e); break; },
                }
            },
            // Transcoder progress
            recv(cmpr_prog_rx) -> msg => {
                match msg {
                    Ok((vid, user_id, msg)) => {
                        user_msg_tx.send(UserMessage {
                                topic: UserMessageTopic::Progress,
                                msg: msg,
                                details: None,
                                user_id: Some(user_id),
                                media_file_id: Some(vid)
                            }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                    },
                    Err(e) => { tracing::warn!("Transcoder is dead ('{:?}'). Exit.", e); break; },
                }
            },
            // Transcoder output
            recv(cmpr_out_rx) -> msg => {
                match msg {
                    Err(e) => { tracing::warn!("Transcoder is dead ('{:?}'). Exit.", e); break; },
                    Ok(res) => {
                        if res.success {
                            let videos_dir = media_files_dir.clone();
                            let db = db.clone();
                            let vid = res.media_file_id.clone();

                            // Thumbnails done?
                            if let Some(thumb_dir) = res.thumb_dir {
                                if let Err(e) = db.conn().and_then(|mut conn| models::MediaFile::set_thumb_sheet_dimensions(&mut conn, &vid, THUMB_SHEET_COLS, THUMB_SHEET_ROWS)) {
                                    tracing::error!(details=%e, "Error storing thumbnail sheet dims in DB");
                                } else {
                                    // Thumbnailer for old media files: find next file to thumbnail, if any
                                    if Some(vid.clone()) == legacy_media_file_now_thumnailing {
                                        legacy_media_file_now_thumnailing = legacy_thumbnail_next_media_file(&db, &videos_dir, &mut cmpr_in_tx.clone());
                                    }
                                }
                                // Write out stdout/stderr to separate files
                                for (name, data) in [("stdout", &res.stdout), ("stderr", &res.stderr)].iter() {
                                    let path = thumb_dir.join(format!("{}.txt", name));
                                    tracing::debug!(media_file=%vid, file=?path, "Writing {} from thumbnailer", name);
                                    match std::fs::write(&path, data) {
                                        Ok(_) => {},
                                        Err(e) => {
                                            tracing::error!(file=?path, details=%e, "Error writing {:?}", name);
                                }}}

                                // Send MediaFileUpdated message to user
                                user_msg_tx.send(UserMessage {
                                        topic: UserMessageTopic::MediaFileUpdated,
                                        msg: "Media thumbnail generated".into(),
                                        details: None,
                                        user_id: Some(res.user_id),
                                        media_file_id: Some(vid.clone())
                                    }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                            }

                            // File done?
                            if let Some(video_dst) = res.video_dst.clone() {
                                // Write out stdout/stderr to separate files
                                for (name, data) in [("stdout", &res.stdout), ("stderr", &res.stderr)].iter() {
                                    let path = videos_dir.join(&vid).join(format!("{}.txt", name));
                                    tracing::debug!(media_file=%vid, file=?path, "Writing {} from ffmpeg", name);
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
                                let user_id = res.dmsg.clone().user_id;
                                let utx = user_msg_tx.clone();
                                let linked_ok = (move || {
                                    let vh_dir = videos_dir.join(&vid);
                                    if !vh_dir.exists() {
                                        if let Err(e) = std::fs::create_dir(&vh_dir) {
                                            tracing::error!(details=%e, "Media dir {:?} was missing after transcoding, and creating it failed. Probably a bug.", vh_dir);
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

                                    if let Err(e) = db.conn().and_then(|mut conn| models::MediaFile::set_recompressed(&mut conn, &vid)) {
                                        tracing::error!(details=%e, "Error marking media file as recompressed in DB");
                                        return false;
                                    } else {
                                        utx.send(UserMessage {
                                            topic: UserMessageTopic::MediaFileUpdated,
                                            msg: "Transcoding done".into(),
                                            details: None,
                                            user_id: Some(user_id),
                                            media_file_id: Some(vid.clone())
                                        }).ok();
                                    }

                                    true
                                })();

                                // Send success message
                                user_msg_tx.send(UserMessage {
                                        topic: if linked_ok {UserMessageTopic::Ok} else {UserMessageTopic::Error},
                                        msg: "Media transcoded.".to_string() + if linked_ok {""} else {" But linking or DB failed."},
                                        details: None,
                                        user_id: Some(res.dmsg.user_id),
                                        media_file_id: Some(res.media_file_id.clone())
                                    }).unwrap_or_else(|e| { tracing::error!(details=%e, "Error sending user message"); });
                            }
                        }
                        else {
                            let msg = format!("Media {} failed", if res.video_dst.is_some() {"transcoding"} else {"thumbnailing"});
                            tracing::error!(video=res.media_file_id, details=?res.dmsg, msg);
                            user_msg_tx.send(UserMessage {
                                    topic: UserMessageTopic::Error,
                                    msg: msg,
                                    details: Some(res.dmsg.details),
                                    user_id: Some(res.dmsg.user_id),
                                    media_file_id: Some(res.media_file_id)
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
