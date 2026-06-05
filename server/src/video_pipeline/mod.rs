//#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

#![allow(unused_parens)]

use std::collections::HashMap;
use std::io::Read;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::path::{PathBuf, Path};

use crossbeam_channel;
use crossbeam_channel::{Receiver, unbounded, select};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use tracing;

use anyhow::{anyhow, Context, bail};
use sha2::{Sha256, Digest};
use hex;

pub mod incoming_monitor;
pub mod metadata_reader;

mod cleanup_rejected;
mod script_processor;

use metadata_reader::MetadataResult;
use crate::api_server::{UserMessage, UserMessageTopic};
use crate::database::error::DBError;
use crate::video_pipeline::metadata_reader::MediaType;
use cleanup_rejected::clean_up_rejected_file;
use crate::database::{DB, models, DbBasicQuery};

#[derive(Debug, Clone)]
pub enum IngestUsernameFrom {
    FileOwner,
    FolderName,
}

impl std::str::FromStr for IngestUsernameFrom {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "file-owner" => Ok(IngestUsernameFrom::FileOwner),
            "folder-name" => Ok(IngestUsernameFrom::FolderName),
            _ => Err(format!("Invalid value '{}', must be 'file-owner' or 'folder-name'", s)),
        }
    }
}

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
        media_id: &str,
        md: &metadata_reader::Metadata,
        data_dir: &Path,
        media_files_dir: &Path,
        target_bitrate: u32,
        db: &DB,
        user_msg_tx: &crossbeam_channel::Sender<UserMessage>,
        cmpr_tx: &crossbeam_channel::Sender<script_processor::CmprInput>,
        transcode_decision_script: &str)
            -> anyhow::Result<bool>
{
    let _span = tracing::info_span!("INGEST_MEDIA",
        media_id = %media_id,
        user=md.user_id,
        filename=%md.src_file.file_name().unwrap_or_default().to_string_lossy()).entered();

    tracing::info!("Ingesting file.");

    let src = PathBuf::from(&md.src_file);
    if !src.is_file() { bail!("Source file not found: {:?}", src) }

    let dir_for_media_file = media_files_dir.join(&media_id);
    tracing::debug!("Media dir = {:?}", dir_for_media_file);

    // File already exists on disk?
    if dir_for_media_file.exists() {
        tracing::debug!("Media dir already exists.");
        match models::MediaFile::get(&mut db.conn()?, &media_id.into()) {
            Ok(v) => {
                let new_owner = &md.user_id;
                if &v.user_id == new_owner {
                    tracing::info!("User already has this media file.");
                    user_msg_tx.send(UserMessage {
                        topic: UserMessageTopic::Ok,
                        msg: "Media file already exists".to_string(),
                        user_id: Some(new_owner.clone()),
                        media_file_id: None,  // Don't pass media file id here, otherwise the pre-existing media would be deleted!
                        ..Default::default()
                    }).ok();

                    clean_up_rejected_file(&data_dir, &src, Some(media_id.into())).unwrap_or_else(|e| {
                        tracing::error!(details=?e, "Cleanup failed.");
                    });

                    return Ok(false);
                } else {
                    bail!("Hash collision?!? Media '{media_id}' already owned by another user '{new_owner}'.")
                }
            },
            Err(DBError::NotFound()) => {
                // File exists, but not in DB. Remove files and reprocess.
                tracing::info!("Dir for '{media_id}' exists, but not in DB. Deleting old dir and reprocessing.");
                std::fs::remove_dir_all(&dir_for_media_file)?;
            }
            Err(e) => {
                bail!("Error checking DB for media file '{}': {}", media_id, e);
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

    // Try rename first (fast, atomic), fallback to copy+delete for cross-filesystem moves
    match std::fs::rename(&src, &src_moved) {
        Ok(_) => {},
        Err(e) if e.raw_os_error() == Some(18) => {
            // EXDEV (cross-device link) - fallback to copy + delete
            tracing::debug!("Cross-filesystem move detected, using copy+delete");
            std::fs::copy(&src, &src_moved)
                .with_context(|| format!("Failed to copy {} to {}", src.display(), src_moved.display()))?;
            std::fs::remove_file(&src)
                .with_context(|| format!("Failed to remove source file {}", src.display()))?;
        },
        Err(e) => return Err(e.into()),
    }

    if !src_moved.exists() { bail!("Failed to move {:?} file to orig/", src_moved) }

    let orig_filename = src.file_name().ok_or(anyhow!("Bad filename: {:?}", src))?.to_string_lossy().into_owned();

    // Add to DB
    tracing::debug!("Adding media file to DB.");
    models::MediaFile::insert(&mut db.conn()?, &models::MediaFileInsert {
        id: media_id.to_string(),
        user_id: md.user_id.clone(),
        media_type: Some(md.media_type.as_ref().into()),
        recompression_done: None,
        thumbs_done: None,
        has_thumbnail: None,
        thumb_sheet_cols: None,
        thumb_sheet_rows: None,
        orig_filename: Some(orig_filename.clone()),
        title: Some(orig_filename),
        total_frames: Some(md.total_frames as i32),
        duration: md.duration.to_f32(),
        fps: Some(md.fps.to_string()),
        raw_metadata_all: Some(md.metadata_all.clone()),
        default_subtitle_id: None,
        version_of: None,
    })?;


    // Check if it needs recompressing by running the decision script
    fn run_transcode_decision_script(
        md: &metadata_reader::Metadata,
        target_max_bitrate: u32,
        script_path: &str
    ) -> anyhow::Result<Option<(String, u32)>> {
        let container = md.src_file.extension()
            .unwrap_or(std::ffi::OsStr::new(""))
            .to_string_lossy()
            .to_lowercase();

        let mut cmd = Command::new(script_path);
        cmd.env("CLAPSHOT_MEDIA_TYPE", md.media_type.as_ref())
           .env("CLAPSHOT_ORIG_CODEC", &md.orig_codec)
           .env("CLAPSHOT_CONTAINER", &container)
           .env("CLAPSHOT_BITRATE", md.bitrate.to_string())
           .env("CLAPSHOT_TARGET_BITRATE", target_max_bitrate.to_string())
           .env("CLAPSHOT_DURATION", md.duration.to_string())
           .env("CLAPSHOT_INPUT_FILE", md.src_file.to_string_lossy().to_string())
           .env("CLAPSHOT_METADATA_JSON", &md.metadata_all);

        tracing::debug!(script=%script_path, "Running transcode decision script");

        let output = cmd.output()
            .with_context(|| format!("Failed to execute transcode decision script: {}", script_path))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Transcode decision script failed (exit code {:?}): {}", output.status.code(), stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        tracing::debug!(output=%stdout, "Transcode decision script output");

        // Parse JSON output
        let json: serde_json::Value = serde_json::from_str(&stdout)
            .with_context(|| format!("Failed to parse transcode decision script JSON output: {}", stdout))?;

        let transcode = json.get("transcode")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow!("Missing or invalid 'transcode' field in script output"))?;

        if transcode {
            let reason = json.get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("transcoding required")
                .to_string();
            let bitrate = json.get("bitrate")
                .and_then(|v| v.as_u64())
                .map(|b| b as u32)
                .unwrap_or(target_max_bitrate);
            Ok(Some((reason, bitrate)))
        } else {
            Ok(None)
        }
    }

    let src = script_processor::CmprInputSource {
        user_id: md.user_id.clone(),
        media_file_id: media_id.to_string(),
        media_type: md.media_type.clone(),
        path: src_moved.clone(),
        duration: md.duration,
    };

    let transcode_req = match run_transcode_decision_script(md, target_bitrate, transcode_decision_script)? {
        Some((reason, new_bitrate)) => {
            let video_dst_prefix = format!("transcoded_br{}_{}", new_bitrate, uuid::Uuid::new_v4());
            cmpr_tx.send(script_processor::CmprInput::Transcode {
                video_dst_dir: dir_for_media_file.clone(),
                video_dst_prefix,
                video_bitrate: new_bitrate,
                src: src.clone()
            }).map(|_| (true, reason)).context("Error sending file to transcoding")
        },
        None => {
            tracing::info!("Media OK already, not transcoding.");
            Ok((false, "".to_string()))
        }
    };

    // Also invoke thumbnail generator unless there was a problem with the file
    if let Ok(_) = &transcode_req {
        let thumb_dir = dir_for_media_file.join("thumbs");
        if let Err(e) = cmpr_tx.send(script_processor::CmprInput::Thumbs {
            thumb_dir,
            thumb_sheet_dims: (THUMB_SHEET_COLS, THUMB_SHEET_ROWS),
            thumb_size: (THUMB_W, THUMB_H),
            src: script_processor::CmprInputSource {
                user_id: md.user_id.clone(),
                media_file_id: media_id.to_string(),
                media_type: md.media_type.clone(),
                path: src_moved.clone(),
                duration: md.duration,
            }
        }) {
            tracing::error!(details=?e, "Failed to send file to thumbnailing");
            if let Err(e) = user_msg_tx.send(UserMessage {
                    topic: UserMessageTopic::Error,
                    msg: "Thumbnailing failed.".to_string(),
                    details: Some(format!("Error sending file to thumbnailing: {}", e)),
                    user_id: Some(md.user_id.clone()),
                    media_file_id: Some(media_id.to_string()),
                    subtitle_id: None,
                    progress: None
                }) { tracing::error!(details=?e, "Failed to send user message") };
        };
    };

    // Tell user about the processing
    match transcode_req {
        Ok((do_transcode, reason)) => {
            // Notify Clapshot Client about the new media file
            user_msg_tx.send(UserMessage {
                topic: UserMessageTopic::MediaFileAdded,
                msg: String::new(),
                details: Some(serde_json::to_string(&md.upload_cookies).map_err(|e| anyhow!("Error serializing cookies: {}", e))?),
                user_id: Some(md.user_id.clone()),
                media_file_id: Some(media_id.to_string()),
                subtitle_id: None,
                progress: None,
            })?;
            // Tell user in text also
            tracing::debug!(transcode=do_transcode, reason=reason, "Media added to DB. Transcode");
            user_msg_tx.send(UserMessage {
                topic: UserMessageTopic::Ok,
                msg: "Media added.".to_string() + if do_transcode {" Transcoding..."} else {""},
                details: if do_transcode { Some(format!("Transcoding because {reason}")) } else { None },
                user_id: Some(md.user_id.clone()),
                media_file_id: Some(media_id.to_string()),
                subtitle_id: None,
                progress: if do_transcode { Some(0.0) } else { None },
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
                media_file_id: Some(media_id.to_string()),
                subtitle_id: None,
                progress: None,
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
    n_workers: usize,
    ingest_username_from: IngestUsernameFrom,
    transcode_script: String,
    thumbnail_script: String,
    transcode_decision_script: String)
{
    tracing::debug!("Starting media file processing pipeline.");

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
                        exit_recvr,
                        ingest_username_from) {
                    tracing::error!(details=?e, "Error from incoming monitor.");
                }});
        (th, incoming_recvr, exit_sender)
    };

    // Thread for the compressor
    let (cmpr_in_tx, cmpr_in_rx) = unbounded::<script_processor::CmprInput>();
    let (cmpr_out_tx, cmpr_out_rx) = unbounded::<script_processor::CmprOutput>();
    let (cmpr_prog_tx, cmpr_prog_rx) = unbounded::<(String, String, String, Option<f32>)>();
    let transcode_script_clone = transcode_script.clone();
    let thumbnail_script_clone = thumbnail_script.clone();
    thread::spawn(move || {
        script_processor::run_forever(cmpr_in_rx, cmpr_out_tx, cmpr_prog_tx, n_workers, transcode_script_clone, thumbnail_script_clone);
    });

    // Migration from older version: find a media file that is missing thumbnail sheet
    fn legacy_thumbnail_next_media_file(db: &DB, videos_dir: &PathBuf, cmpr_in: &mut crossbeam_channel::Sender<script_processor::CmprInput>) -> Option<String> {

        let candidates = db.conn()
            .and_then(|mut conn| models::MediaFile::get_all_with_missing_thumbnails(&mut conn))
            .map_err(|e| { tracing::error!(details=?e, "DB: Failed to get media files without thumbnails."); }).ok()?;

        if let Some(v) = candidates.first() {
            tracing::info!(id=%v.id, "Found legacy media file that needs thumbnailing.");

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
                    let media_type = v.media_type.as_ref().map(|mt| MediaType::from_str(mt)).transpose().ok().flatten()
                        .or_else(|| { tracing::error!(media_file_id=%v.id, "Legacy thumbnailing failed. Media type missing or unkwnown: {:?}", v.media_type); None })?;

                    let req = script_processor::CmprInput::Thumbs {
                        thumb_dir: videos_dir.join(&v.id).join("thumbs"),
                        thumb_sheet_dims: (THUMB_SHEET_COLS, THUMB_SHEET_ROWS),
                        thumb_size: (THUMB_W, THUMB_H),
                        src: script_processor::CmprInputSource {
                            user_id: v.user_id.clone(),
                            media_file_id: v.id.clone(),
                            media_type,
                            path: file_path,
                            duration: Decimal::from_f32(v.duration.unwrap_or(0.0)).unwrap_or_default(),
                        },
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
                        tracing::debug!("Got upload result. Submitting it for processing. {:?}", msg);
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
                                        let ing_res = ingest_media_file(&vid, &md, &data_dir, &media_files_dir, target_bitrate, &db, &user_msg_tx, &cmpr_in_tx, &transcode_decision_script).map_err(|e| {
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
                                    media_file_id: vid,
                                    subtitle_id: None,
                                    progress: None
                                }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                        }
                    },
                    Err(e) => { tracing::debug!("Metadata reader disconnected ('{:?}'). Exit.", e); break; },
                }
            },
            // Incoming file from monitor
            recv(from_mon) -> msg => {
                match msg {
                    Ok(new_file) => {
                        // Create user if it does not exist (to store notifications for when user comes online)
                        if let Ok(mut conn) = db.conn() {
                            if let Err(e) = models::User::get_or_create(&mut conn, &new_file.user_id, None) {
                                tracing::error!("Error getting or creating user '{}': {:?}", new_file.user_id, e);
                                // Don't crash the pipeline, but log the error
                            };
                        };

                        // Relay to metadata reader
                        to_md.send(new_file).unwrap_or_else(|e| {
                            tracing::error!("FATAL. Error sending file to metadata reader: {:?}", e);
                            terminate_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        });
                    },
                    Err(e) => {
                        tracing::debug!("Metadata reader disconnected ('{:?}'). Exit.", e); break; },
                }
            },
            // Transcoder progress
            recv(cmpr_prog_rx) -> msg => {
                match msg {
                    Ok((vid, user_id, msg, progress)) => {
                        tracing::trace!("Posting transcoder progress: {:?} {:?} {:?} {:?}", vid, user_id, msg, progress);
                        user_msg_tx.send(UserMessage {
                                topic: UserMessageTopic::Progress,
                                msg,
                                details: None,
                                user_id: Some(user_id),
                                media_file_id: Some(vid),
                                subtitle_id: None,
                                progress
                            }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                    },
                    Err(e) => { tracing::warn!("Transcoder is dead ('{:?}'). Exit.", e); break; },
                }
            },
            // Transcoder output
            recv(cmpr_out_rx) -> msg => {
                use script_processor::CmprOutput::{TranscodeSuccess, ThumbsSuccess, TranscodeFailure, ThumbsFailure};
                match msg {
                    Err(e) => { tracing::warn!("Transcoder is dead ('{:?}'). Exit.", e); break; },
                    Ok(res) => match &res {

                        TranscodeSuccess { video_dst, logs } =>
                        {
                            let videos_dir = media_files_dir.clone();
                            let vid = logs.media_file_id.clone();

                            tracing::info!(media_file=%vid, log_info=%logs.stdout, "Transcoding completed");

                            // Get filename from path
                            fn get_filename(p: &PathBuf) -> anyhow::Result<String> {
                                Ok(p.file_name().ok_or(anyhow!("bad filename: {}", p.to_string_lossy()))?.to_str().ok_or(anyhow!("bad encoding"))?.to_string())
                            }

                            // Symlink to transcoded file
                            let user_id = logs.dmsg.clone().user_id;
                            let utx = user_msg_tx.clone();
                            let db = db.clone();
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
                                    tracing::error!(details=%e, "Failed to create symlink {:?} -> {:?}", symlink_path, video_dst);
                                    return false;
                                }

                                // Refresh fps/total_frames from the transcoded output. The source file's
                                // values are wrong when the playable video is synthesized (audio waveform
                                // videos are rendered at a fixed fps; the source audio has no fps at all),
                                // and the client computes SMPTE timecodes from these values.
                                match metadata_reader::read_fps_and_frame_count(video_dst) {
                                    Ok((out_fps, out_frames)) => {
                                        if let Err(e) = db.conn().and_then(|mut conn|
                                            models::MediaFile::set_fps_and_frame_count(&mut conn, &vid, &out_fps.to_string(), out_frames as i32))
                                        {
                                            tracing::error!(details=%e, "Error updating fps/total_frames in DB after transcoding");
                                            return false;
                                        }
                                    },
                                    Err(e) => {
                                        tracing::warn!(details=%e, "Could not read fps/frame count from transcoded output; keeping source values");
                                    }
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
                                        media_file_id: Some(vid.clone()),
                                        subtitle_id: None,
                                        progress: None
                                    }).ok();
                                }

                                true
                            })();

                            // Send success message
                            user_msg_tx.send(UserMessage {
                                    topic: if linked_ok {UserMessageTopic::Ok} else {UserMessageTopic::Error},
                                    msg: "Media transcoded.".to_string() + if linked_ok {""} else {" But linking or DB failed."},
                                    details: None,
                                    user_id: Some(logs.dmsg.user_id.clone()),
                                    media_file_id: Some(logs.media_file_id.clone()),
                                    subtitle_id: None,
                                    progress: Some(1.0)
                                }).unwrap_or_else(|e| { tracing::error!(details=%e, "Error sending user message"); });
                        },

                        ThumbsSuccess { thumb_dir, thumb_sheet_dims, logs } =>
                        {
                            let videos_dir = media_files_dir.clone();
                            let vid = logs.media_file_id.clone();
                            let mut db_errors = false;

                            // Thumbnails (and/or sheet) done?
                            if let Some(_thumb_dir) = thumb_dir {

                                tracing::info!(media_file=%vid, log_info=%logs.stdout, "Thumbnailing completed");

                                // Set has thumbnail in DB
                                if let Err(e) = db.conn().and_then(|mut conn| models::MediaFile::set_has_thumb(&mut conn, &vid, true)) {
                                    tracing::error!(details=%e, "Error in set_has_thumb to DB");
                                    db_errors = true;
                                }

                                // Thumb sheet done?
                                if let Some((sheet_cols, sheet_rows)) = thumb_sheet_dims {
                                    if let Err(e) = db.conn().and_then(|mut conn| models::MediaFile::set_thumb_sheet_dimensions(&mut conn, &vid, *sheet_cols, *sheet_rows)) {
                                        tracing::error!(details=%e, "Error storing thumbnail sheet dims in DB");
                                        db_errors = true;
                                    } else {
                                        // Thumbnailer for old media files: find next file to thumbnail, if any
                                        if Some(vid.clone()) == legacy_media_file_now_thumnailing {
                                            legacy_media_file_now_thumnailing = legacy_thumbnail_next_media_file(&db, &videos_dir, &mut cmpr_in_tx.clone());
                                        }
                                    }
                                }

                                // Send MediaFileUpdated message to user
                                user_msg_tx.send(UserMessage {
                                    topic: UserMessageTopic::MediaFileUpdated,
                                    msg: "Media thumbnail generated".into(),
                                    details: None,
                                    user_id: Some(logs.user_id.clone()),
                                    media_file_id: Some(vid.clone()),
                                    subtitle_id: None,
                                    progress: None
                                }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                            }

                            // Set thumbs_done in DB anyway.
                            // If thumb_dir is None, it means no thumbnails were generated, but the process is done (e.g. for audio files)
                            if !db_errors {  // If there were DB errors - they should be retried later
                                if let Err(e) = db.conn().and_then(|mut conn| models::MediaFile::set_thumbs_done(&mut conn, &vid)) {
                                    tracing::error!(details=%e, "Error storing thumbs_done in DB");
                                }
                            }
                        },

                        TranscodeFailure { logs, .. } |
                        ThumbsFailure { logs } =>
                        {
                            let op = if matches!(&res, TranscodeFailure {..}) { "transcoding" } else { "thumbnailing" };
                            let msg = format!("Media {op} failed");
                            let logs = logs.clone();
                            tracing::error!(video=logs.media_file_id, details=?logs.dmsg, log_location=%logs.stdout, msg);
                            user_msg_tx.send(UserMessage {
                                    topic: UserMessageTopic::Error,
                                    msg: msg.clone(),
                                    details: Some(format!("{}. {}. Check server logs for log file location.", logs.dmsg.details, logs.stdout)),
                                    user_id: Some(logs.dmsg.user_id),
                                    media_file_id: Some(logs.media_file_id),
                                    subtitle_id: None,
                                    progress: None
                                }).unwrap_or_else(|e| { tracing::error!("Error sending user message: {:?}", e); });
                        },
                    }
                }
            }
        }

        if mon_thread.is_finished() {
            tracing::error!("Incoming monitor finished. Exit.");
            break;
        }
        if terminate_flag.load(std::sync::atomic::Ordering::Relaxed) {
            tracing::debug!("Termination flag set. Exit.");
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
