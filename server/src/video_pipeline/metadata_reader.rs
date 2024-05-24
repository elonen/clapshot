use std::{collections::HashMap, process::Command};
use std::sync::atomic::Ordering;
use threadpool::ThreadPool;
use std::path::PathBuf;
use serde_json;
use crossbeam_channel::{Sender, Receiver, RecvError};
use tracing;
use rust_decimal::prelude::*;
use std::sync::atomic::AtomicBool;
use std::str::FromStr;
use super::{IncomingFile, DetailedMsg};

#[derive(Debug, Clone)]
pub enum MediaType {
    Video,
    Audio,
    Image,
}
impl AsRef<str> for MediaType {
    fn as_ref(&self) -> &str {
        match self {
            MediaType::Video => "video",
            MediaType::Audio => "audio",
            MediaType::Image => "image",
        }
    }
}
impl FromStr for MediaType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "video" => Ok(MediaType::Video),
            "audio" => Ok(MediaType::Audio),
            "image" => Ok(MediaType::Image),
            _ => Err(()),
        }
    }
}


#[derive(Debug, Clone)]
pub struct Metadata {
    pub src_file: PathBuf,
    pub user_id: String,
    pub total_frames: u32,
    pub duration: Decimal,
    pub media_type: MediaType,
    pub orig_codec: String,
    pub fps: Decimal,
    pub bitrate: u32,
    pub metadata_all: String,
    pub upload_cookies: HashMap<String, String>    // Cookies from the upload, not read from the file
}

pub type MetadataResult = Result<Metadata, DetailedMsg>;

/// Run Mediainfo shell command and return the output
///
/// # Arguments
/// * `file_path` - Path to the file to be analyzed
fn run_mediainfo( file: &PathBuf ) -> Result<serde_json::Value, String>
{
    // Link to source file to a temporary file to avoid problems with
    // special characters in the path with mediainfo
    let uuid = uuid::Uuid::new_v4();
    let file_dir = file.parent().ok_or("Failed to get parent directory")?;
    let temp_dir = file_dir.join(uuid.to_string());
    let link_path = temp_dir.join(format!("tempname"));

    // (symlink wasn't reliable on Windows WSL, so we'll use hard link instead)
    tracing::debug!("Creating temp hard link from {:?} to {:?}", file, link_path);
    std::fs::create_dir(&temp_dir).map_err(|e| e.to_string())?;
    std::fs::hard_link(file, &link_path).map_err(|e| e.to_string())?;

    // Run mediainfo
    let cmd = &mut Command::new("mediainfo");
    cmd.arg("--Output=JSON").arg("--").arg(&link_path);
    tracing::info!("Calling mediainfo");
    tracing::debug!("Exec: {:?}", cmd);
    let mediainfo_res = cmd.output();

    // Remove temp hardlink
    tracing::debug!("Removing temp hard link and directory ({:?})", link_path);
    if let Err(e) = std::fs::remove_file(&link_path) {
        tracing::error!("Failed to remove temporary link file: {}", e);
    } else {
        if let Err(e) = std::fs::remove_dir(&temp_dir) {
            tracing::error!("Failed to remove temporary directory: {}", e);
        }
    }

    match mediainfo_res
    {
        Ok(output) => {
            if output.status.success() {
                {
                    let json_res = String::from_utf8(output.stdout)
                        .map_err(|e| e.to_string())?;
                    serde_json::from_str(&json_res)
                }.map_err(|e| format!("Error parsing mediainfo JSON: {:?}", e))
            } else {
                tracing::error!("Mediainfo stdout: {}", String::from_utf8_lossy(&output.stdout));
                tracing::error!("Mediainfo stderr: {}", String::from_utf8_lossy(&output.stderr));
                Err( format!("Mediainfo exited with error: {}",
                    String::from_utf8_lossy(&output.stderr)))
            }
        },
        Err(e) => {
            Err(format!("Failed to execute mediainfo: {}", e))
        }
    }
}

/// Parse mediainfo JSON output and return the metadata object.
/// Possibly returned error message contains details to be sent to the client
/// in the DetailedMsg struct.
///
/// # Arguments
/// * `json` - Mediainfo JSON output
/// * `args` - Metadata request arguments
/// * `get_file_size` - Closure to get the file size (only called if bitrate is not available and we need to calculate it)
fn extract_variables<F>(json: serde_json::Value, args: &IncomingFile, get_file_size: F) -> Result<Metadata, String>
    where F: FnOnce() -> Result<u64, String>
{
    let tracks = json["media"]["track"].as_array().ok_or("No media tracks found")?;

    // Video file
    if let Some(video_track) = tracks.iter().find(|t| t["@type"] == "Video") {

        // Bitrate is tricky. It might be in "BitRate" or "BitRate_Nominal". If it's not in either, we'll estimate it.
        let duration = Decimal::from_str(video_track["Duration"].as_str().ok_or("Duration not found")?).map_err(|_| "Invalid duration")?;
        let bitrate = {
            let bitrate_str = video_track["BitRate"].as_str()
                .or(video_track["BitRate_Nominal"].as_str());
            match bitrate_str {
                Some(bit_rate_str) => bit_rate_str.parse().map_err(|_| format!("Invalid bitrate: {}", bit_rate_str))?,
                None => {
                    let duration = duration.to_f32().ok_or("Invalid duration")?;
                    ((get_file_size()? as f32) * 8.0 / duration) as u32
                }}};

        Ok(Metadata {
            src_file: args.file_path.clone(),
            user_id: args.user_id.clone(),
            total_frames: video_track["FrameCount"].as_str().ok_or("FrameCount not found")?.parse().map_err(|_| "Invalid frame count".to_string())?,
            duration,
            media_type: MediaType::Video,
            orig_codec: video_track["Format"].as_str().ok_or("No codec found")?.to_string(),
            fps: Decimal::from_str(video_track["FrameRate"].as_str().ok_or("FPS not found")?).map_err(|_| "Invalid FPS".to_string())?,
            bitrate,
            metadata_all: json.to_string(),
            upload_cookies: args.cookies.clone()
        })
    }

    // Audio file
    else if let Some(audio_track) = tracks.iter().find(|t| t["@type"] == "Audio") {
        Ok(Metadata {
            src_file: args.file_path.clone(),
            user_id: args.user_id.clone(),
            total_frames: 0,
            duration: Decimal::from_str(audio_track["Duration"].as_str().ok_or("Duration not found")?).map_err(|_| "Invalid duration".to_string())?,
            media_type: MediaType::Audio,
            orig_codec: audio_track["Format"].as_str().ok_or("No codec found")?.to_string(),
            fps: Decimal::from_u8(0).unwrap(),
            bitrate: audio_track["BitRate"].as_str().ok_or("Bitrate not found")?.parse().map_err(|_| "Invalid bitrate".to_string())?,
            metadata_all: json.to_string(),
            upload_cookies: args.cookies.clone()
        })
    }

    // Image file
    else if let Some(image_track) = tracks.iter().find(|t| t["@type"] == "Image") {
        Ok(Metadata {
            src_file: args.file_path.clone(),
            user_id: args.user_id.clone(),
            total_frames: 1,
            duration: Decimal::from_u8(0).unwrap(),
            media_type: MediaType::Image,
            orig_codec: image_track["Format"].as_str().ok_or("No codec found")?.to_string(),
            fps: Decimal::from_u8(0).unwrap(),
            bitrate: 0,
            metadata_all: json.to_string(),
            upload_cookies: args.cookies.clone()
        })
    } else {
        return Err("No video, audio or image track found".to_string());
    }
}

/// Run mediainfo and extract the metadata
fn read_metadata_from_file(args: &IncomingFile) -> Result<Metadata, String>
{
    let json = run_mediainfo(&args.file_path)?;
    extract_variables(json, args, || Ok(args.file_path.metadata().map_err(|e| format!("Failed to get file size: {:?}", e))?.len()))
}

/// Listens to inq for new files to scan for metadata with Mediainfo shell command.
/// When a new file is received, it is processed and the result is sent to outq.
/// Starts a thread pool of `n_workers` workers to support simultaneous processing of multiple files.
/// Exits when inq is closed or outq stops accepting messages.
///
/// # Arguments
/// * `inq` - channel to receive new files to process
/// * `outq` - channel to send results to
/// * `n_workers` - number of threads to use for processing
pub fn run_forever(inq: Receiver<IncomingFile>, outq: Sender<MetadataResult>, n_workers: usize)
{
    let _span = tracing::info_span!("MD").entered();
    tracing::info!(n_workers = n_workers, "Starting.");

    let pool = ThreadPool::new(n_workers);
    let pool_is_healthy  = std::sync::Arc::new(AtomicBool::new(true));

    while pool_is_healthy.load(Ordering::Relaxed) {
        match inq.recv() {
            Ok(args) => {
                tracing::info!(file=%args.file_path.display(), user=args.user_id, "Scanning file.");
                let pool_is_healthy = pool_is_healthy.clone();
                let outq = outq.clone();
                pool.execute(move || {
                    if let Err(e) = outq.send(
                        read_metadata_from_file(&args).map_err(|e| {
                                DetailedMsg {
                                    msg: "Metadata read failed".to_string(),
                                    details: e,
                                    src_file: args.file_path.clone(),
                                    user_id: args.user_id.clone() }}))
                    {
                        tracing::error!(details=%e, "Result send failed! Aborting.");
                        pool_is_healthy.store(false, Ordering::Relaxed);
                    }});
            },
            Err(RecvError) => {
                tracing::info!("Incoming queue closed.");
                break;
            }
        }
    }

    tracing::debug!("Exiting.");
}


// Unit tests =====================================================================================

#[cfg(test)]
fn test_fixture(has_bitrate: bool, has_fps: bool) -> (IncomingFile, serde_json::Value)
{
    let bitrate = if has_bitrate { r#", "BitRate": "1000""# } else { "" };
    let fps = if has_fps { r#", "FrameRate": "30""# } else { "" };

    let json = serde_json::from_str(&format!(r#"{{
        "media": {{ "track": [ {{
                    "@type": "Video",  "FrameCount": "100",
                    "Duration": "5.0", "Format": "H264"
                    {}{}
                }} ] }} }}"#, bitrate, fps)).unwrap();

    let args = IncomingFile {
        file_path: PathBuf::from("test.mp4"),
        user_id: "test_user".to_string(),
        cookies: Default::default()
    };

    (args, json)
}

#[test]
fn test_extract_variables_ok()
{
    let (args, json) = test_fixture(true, true);
    let metadata = extract_variables(json, &args, || Ok(1000)).unwrap();
    assert_eq!(metadata.total_frames, 100);
    assert_eq!(metadata.duration, Decimal::from_str("5").unwrap());
    assert_eq!(metadata.orig_codec, "H264");
    assert_eq!(metadata.fps, Decimal::from_str("30.000").unwrap());
    assert_eq!(metadata.bitrate, 1000);
}

#[test]
fn test_extract_variables_missing_bitrate()
{
    let (args, json) = test_fixture(false, true);
    let metadata = extract_variables(json, &args, || Ok(1000)).unwrap();
    assert_eq!(metadata.bitrate, 1000*8/5);
}

#[test]
fn test_extract_variables_fail_missing_fps()
{
    let (args, json) = test_fixture(true, false);
    let metadata = extract_variables(json, &args, || Ok(1000));
    assert!(metadata.is_err());
    assert!(metadata.unwrap_err().to_lowercase().contains("fps"));
}
