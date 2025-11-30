use std::{process::Command, io::BufRead, collections::HashMap};
use std::path::PathBuf;
use crossbeam_channel::{Sender, Receiver};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use tracing;
use threadpool::ThreadPool;
use std::fs;
use chrono;

use super::metadata_reader::MediaType;
use super::DetailedMsg;

pub type ProgressSender = crossbeam_channel::Sender<(String, String, String, Option<f32>)>;

// Input to the script processor
#[derive(Debug, Clone)]
pub enum CmprInput {
    Transcode {
        video_dst_dir: PathBuf,  // Directory where script should output
        video_dst_prefix: String, // Filename prefix (script decides extension)
        video_bitrate: u32,
        src: CmprInputSource,
    },
    Thumbs {
        thumb_dir: PathBuf,
        thumb_sheet_dims: (u32, u32),
        thumb_size: (u32, u32),
        src: CmprInputSource,
    }
}

#[derive(Debug, Clone)]
pub struct CmprInputSource {
    pub user_id: String,
    pub media_file_id: String,
    pub media_type: MediaType,
    pub path: PathBuf,
    pub duration: Decimal,
}

// Output from the script processor
#[derive(Debug, Clone)]
pub enum CmprOutput {
    TranscodeSuccess {
        video_dst: PathBuf,  // Final output file path (determined by script)
        logs: CmprLogs
    },
    ThumbsSuccess {
        thumb_dir: Option<PathBuf>,
        thumb_sheet_dims: Option<(u32, u32)>,
        logs: CmprLogs
    },
    TranscodeFailure { logs: CmprLogs },
    ThumbsFailure { logs: CmprLogs }
}

#[derive(Debug, Clone)]
pub struct CmprLogs {
    pub media_file_id: String,
    pub user_id: String,
    pub stdout: String,
    pub _stderr: String,
    pub dmsg: DetailedMsg,
}


/// Validate and sanitize values passed to scripts via environment variables
fn validate_env_value(key: &str, value: &str) -> Result<String, String> {
    match key {
        "CLAPSHOT_TARGET_BITRATE" => {
            // Bitrate should be numeric only
            if value.chars().all(|c| c.is_numeric()) {
                Ok(value.to_string())
            } else {
                Err(format!("Invalid bitrate format: {}", value))
            }
        },
        "CLAPSHOT_MEDIA_TYPE" => {
            // Media type should be one of known values
            match value {
                "video" | "audio" | "image" => Ok(value.to_string()),
                _ => Err(format!("Invalid media type: {}", value))
            }
        },
        "CLAPSHOT_USER_ID" | "CLAPSHOT_MEDIA_ID" => {
            // User/media IDs should be alphanumeric + basic chars only
            if value.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
                Ok(value.to_string())
            } else {
                Err(format!("Invalid {} format: {}", key, value))
            }
        },
        "CLAPSHOT_DURATION" => {
            // Duration should be numeric (can have decimal point)
            if value.chars().all(|c| c.is_numeric() || c == '.') {
                Ok(value.to_string())
            } else {
                Err(format!("Invalid duration format: {}", value))
            }
        },
        "CLAPSHOT_THUMB_SIZE" | "CLAPSHOT_SHEET_DIMS" => {
            // Dimensions should be in format "NxN" where N is numeric
            if value.matches('x').count() == 1 &&
               value.split('x').all(|part| part.chars().all(|c| c.is_numeric()) && !part.is_empty()) {
                Ok(value.to_string())
            } else {
                Err(format!("Invalid dimension format (expected NxN): {}", value))
            }
        },
        "CLAPSHOT_OUTPUT_PREFIX" => {
            // Output prefix should be alphanumeric + basic chars only (no path separators)
            if value.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
                Ok(value.to_string())
            } else {
                Err(format!("Invalid output prefix format: {}", value))
            }
        },
        _ => {
            // For file paths, allow basic path chars but reject dangerous sequences
            if value.contains("..") || value.contains(";") || value.contains("|") || value.contains("&") ||
               value.contains("`") || value.contains("$") || value.contains("'") || value.contains("\"") ||
               value.contains("\\") || value.contains("\n") || value.contains("\r") {
                Err(format!("Potentially unsafe value for {}: {}", key, value))
            } else if !value.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '.' || c == ' ') {
                Err(format!("Invalid characters in {}: {}", key, value))
            } else {
                Ok(value.to_string())
            }
        }
    }
}

/// Create a sanitized symlink in the orig directory for script access
fn create_sanitized_symlink(src_path: &PathBuf) -> Result<PathBuf, String> {
    let orig_dir = src_path.parent()
        .ok_or("Source file has no parent directory")?;

    let extension = src_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");

    // Use a unique name per run to avoid conflicts with stale symlinks
    let unique_id = uuid::Uuid::new_v4().to_string();
    let sanitized_name = format!("orig_{}.{}", unique_id, extension);
    let sanitized_path = orig_dir.join(sanitized_name);

    tracing::debug!(src=?src_path, target=?sanitized_path, "Creating sanitized symlink");

    // Check if source file exists
    if !src_path.exists() {
        return Err(format!("Source file does not exist: {}", src_path.display()));
    }

    // Always create a fresh symlink (remove any existing one first)
    if sanitized_path.exists() {
        if let Err(e) = fs::remove_file(&sanitized_path) {
            return Err(format!("Failed to remove existing symlink: {}", e));
        }
    }

    // Use relative path for symlink target (just the filename) since both files are in the same directory
    let src_filename = src_path.file_name()
        .ok_or("Source file has no filename")?;

    if let Err(e) = std::os::unix::fs::symlink(src_filename, &sanitized_path) {
        return Err(format!("Failed to create sanitized symlink from {} to {}: {}",
                          src_filename.to_string_lossy(), sanitized_path.display(), e));
    }

    tracing::debug!(src=?src_path, symlink=?sanitized_path, "Created sanitized symlink");
    Ok(sanitized_path)
}

/// Set up environment variables for script execution
fn setup_script_environment(src: &CmprInputSource, input_file: &PathBuf, output_dir: &PathBuf,
                           output_prefix: &str, target_bitrate: u32, progress_pipe: &Option<String>)
                           -> Result<HashMap<String, String>, String> {
    let mut env_vars = HashMap::new();

    // Validate and set environment variables
    env_vars.insert("CLAPSHOT_INPUT_FILE".to_string(),
                   validate_env_value("CLAPSHOT_INPUT_FILE", &input_file.to_string_lossy())?);
    env_vars.insert("CLAPSHOT_OUTPUT_DIR".to_string(),
                   validate_env_value("CLAPSHOT_OUTPUT_DIR", &output_dir.to_string_lossy())?);
    env_vars.insert("CLAPSHOT_OUTPUT_PREFIX".to_string(),
                   validate_env_value("CLAPSHOT_OUTPUT_PREFIX", output_prefix)?);
    env_vars.insert("CLAPSHOT_MEDIA_TYPE".to_string(),
                   validate_env_value("CLAPSHOT_MEDIA_TYPE", src.media_type.as_ref())?);
    env_vars.insert("CLAPSHOT_TARGET_BITRATE".to_string(),
                   validate_env_value("CLAPSHOT_TARGET_BITRATE", &target_bitrate.to_string())?);
    env_vars.insert("CLAPSHOT_USER_ID".to_string(),
                   validate_env_value("CLAPSHOT_USER_ID", &src.user_id)?);
    env_vars.insert("CLAPSHOT_MEDIA_ID".to_string(),
                   validate_env_value("CLAPSHOT_MEDIA_ID", &src.media_file_id)?);
    env_vars.insert("CLAPSHOT_DURATION".to_string(),
                   validate_env_value("CLAPSHOT_DURATION", &src.duration.to_string())?);

    if let Some(pipe) = progress_pipe {
        env_vars.insert("CLAPSHOT_PROGRESS_PIPE".to_string(),
                       validate_env_value("CLAPSHOT_PROGRESS_PIPE", pipe)?);
    }

    Ok(env_vars)
}

/// Find the actual output file created by the script
fn find_script_output(output_dir: &PathBuf, output_prefix: &str) -> Result<PathBuf, String> {
    let entries = fs::read_dir(output_dir)
        .map_err(|e| format!("Failed to read output directory: {}", e))?;

    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                // Must start with exact prefix and have a known video extension
                if filename.starts_with(output_prefix) {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let ext_lower = ext.to_lowercase();
                        if ["mp4", "mkv", "webm", "avi", "mov"].contains(&ext_lower.as_str()) {
                            candidates.push(path);
                        }
                    }
                }
            }
        }
    }

    match candidates.len() {
        0 => Err(format!("No valid output files found with prefix: {}", output_prefix)),
        1 => Ok(candidates.into_iter().next().unwrap()),
        _ => Err(format!("Multiple valid output files found with prefix {}: {:?}", output_prefix, candidates))
    }
}

fn err2cout<E: std::fmt::Debug>(msg_txt: &str, err: E, args: &CmprInput, sanitized_symlink: Option<&PathBuf>) -> CmprOutput {
    let details_str = format!("{:?}", err);
    tracing::error!(details=&details_str, "err2cout: {}", msg_txt);

    // Clean up sanitized symlink if provided
    if let Some(symlink_path) = sanitized_symlink {
        if let Err(e) = fs::remove_file(symlink_path) {
            tracing::debug!(details=%e, symlink=?symlink_path, "Failed to remove sanitized symlink during error cleanup (may not exist)");
        }
    }

    let src = match args {
        CmprInput::Transcode { src, .. } | CmprInput::Thumbs { src, .. } => src,
    };

    let logs = CmprLogs {
        media_file_id: src.media_file_id.clone(),
        user_id: src.user_id.clone(),
        stdout: "".into(),
        _stderr: "".into(),
        dmsg: DetailedMsg {
            msg: msg_txt.to_string(),
            details: details_str,
            src_file: src.path.clone(),
            user_id: src.user_id.clone()
        }
    };
    match args {
        CmprInput::Transcode { .. } => { CmprOutput::TranscodeFailure { logs } },
        CmprInput::Thumbs { .. } => { CmprOutput::ThumbsFailure { logs } }
    }
}

/// Run transcoding script and return the output
fn run_transcode_script(src: &CmprInputSource, output_dir: PathBuf, output_prefix: String,
                       video_bitrate: u32, progress: ProgressSender, script_path: &str) -> CmprOutput {
    let _span = tracing::info_span!("run_transcode_script",
        media_file = %src.media_file_id,
        user = %src.user_id,
        thread = ?std::thread::current().id()).entered();

    // Create sanitized symlink for script access
    let sanitized_input = match create_sanitized_symlink(&src.path) {
        Ok(path) => path,
        Err(e) => return err2cout("Failed to create sanitized symlink", e,
                                 &CmprInput::Transcode {
                                     video_dst_dir: output_dir,
                                     video_dst_prefix: output_prefix,
                                     video_bitrate,
                                     src: src.clone()
                                 }, None)
    };

    // Create transcoded/ subdirectory for script to work in
    let script_work_dir = output_dir.join("transcoded");
    if let Err(e) = fs::create_dir_all(&script_work_dir) {
        return err2cout("Failed to create script work directory", e,
                       &CmprInput::Transcode {
                           video_dst_dir: output_dir,
                           video_dst_prefix: output_prefix,
                           video_bitrate,
                           src: src.clone()
                       }, Some(&sanitized_input));
    }

    // Set up progress pipe in a temporary directory (not user-writable space)
    let temp_dir = std::env::temp_dir();
    let unique_pipe_id = uuid::Uuid::new_v4().to_string();
    let progress_pipe_path = temp_dir.join(format!("clapshot_progress_{}.pipe", unique_pipe_id));
    let ppipe_fname = match progress_pipe_path.to_str() {
        None => None,
        Some(fname) => unix_named_pipe::create(&fname, None)
            .map(|_| fname.to_string())
            .map_err(|e| e.to_string())
            .map_or_else(|e| {
                tracing::warn!(details=e, "Won't track script progress; failed to create pipe file.");
                None
            }, |f| Some(f))
    };

    let progress_terminate = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Thread to read script progress reports (assume same format as ffmpeg)
    let progress_thread = {
        let progress_terminate = progress_terminate.clone();
        let user_id = src.user_id.clone();
        let vid = src.media_file_id.clone();

        // Extract duration before spawning the thread to avoid lifetime issues
        let total_duration_us = (src.duration.to_f64().unwrap_or(0.0) * 1_000_000.0) as u64;

        match ppipe_fname.clone() {
            None => std::thread::spawn(move || {}), // No pipe, skip progress tracking
            Some(pfn) => {
                std::thread::spawn(move || {
                    let _span = tracing::info_span!("script_progress",
                    thread = ?std::thread::current().id()).entered();

                    let f = match unix_named_pipe::open_read(&pfn) {
                        Ok(f) => f,
                        Err(e) => {
                            tracing::error!(details=%e, "Failed to open progress pipe.");
                            return;
                        }
                    };
                    let reader = &mut std::io::BufReader::new(&f);

                    let mut msg: Option<String> = None;
                    let mut done_ratio = None;
                    let mut current_time_us: Option<u64> = None;

                    while !progress_terminate.load(std::sync::atomic::Ordering::Relaxed) {
                        tracing::trace!("Reading progress from script pipe...");
                        match reader.lines().next() {
                            Some(Err(e)) => {
                                if e.kind() != std::io::ErrorKind::WouldBlock {
                                    tracing::error!(details=%e, "Failed to read from progress pipe.");
                                    break;
                                } else {
                                    std::thread::sleep(std::time::Duration::from_millis(250));
                                }
                            },
                            None => {
                                tracing::debug!("Progress pipe EOF. Sleeping...");
                                std::thread::sleep(std::time::Duration::from_millis(250));
                            }
                            Some(Ok(l)) => {
                                tracing::trace!(chunk=%l, "Got script progress line.");
                                // Parse ffmpeg-style progress format: key=value
                                if let Some(idx) = l.find("=") {
                                    let (key, val) = l.split_at(idx);
                                    let val = &val[1..];
                                    match key {
                                        "progress" => {
                                            match val {
                                                "end" => {
                                                    msg = Some("Transcoding done.".to_string());
                                                    done_ratio = Some(1.0);
                                                },
                                                "continue" => {
                                                    msg = Some("Transcoding...".to_string());
                                                    // Calculate progress percentage if we have both current time and duration
                                                    if let Some(time_us) = current_time_us {
                                                        if total_duration_us > 0 {
                                                            let progress_pct = (time_us as f32 / total_duration_us as f32).min(1.0).max(0.0);
                                                            done_ratio = Some(progress_pct);
                                                        }
                                                    }
                                                },
                                                _ => {
                                                    msg = Some("Transcoding...".to_string());
                                                }
                                            }
                                        },
                                        "out_time_us" => {
                                            // Parse current position in microseconds
                                            if let Ok(time_us) = val.parse::<u64>() {
                                                current_time_us = Some(time_us);
                                            }
                                        },
                                        _ => {} // Ignore other keys
                                    }
                                }

                                // Send progress message (if any)
                                if let Some(msg) = msg.take() {
                                    if let Err(e) = progress.send((vid.clone(), user_id.clone(), msg, done_ratio.clone())) {
                                        tracing::debug!(details=%e, "Failed to send script progress message. Ending progress tracking.");
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    tracing::debug!("Progress thread terminating.");
                })
            }
        }
    };

    // Set up environment variables for script
    let env_vars = match setup_script_environment(src, &sanitized_input, &script_work_dir, &output_prefix,
                                                  video_bitrate, &ppipe_fname) {
        Ok(vars) => vars,
        Err(e) => return err2cout("Failed to set up script environment", e,
                                 &CmprInput::Transcode {
                                     video_dst_dir: output_dir,
                                     video_dst_prefix: output_prefix,
                                     video_bitrate,
                                     src: src.clone()
                                 }, Some(&sanitized_input))
    };

    // Run the transcoding script
    let script_path_owned = script_path.to_string();
    let media_id = src.media_file_id.clone();
    let user_id = src.user_id.clone();
    let input_file = sanitized_input.clone();
    let output_dir_clone = output_dir.clone();
    let script_thread = {
        std::thread::spawn(move || {
            let _span = tracing::info_span!("transcode_script",
                thread = ?std::thread::current().id()).entered();

            let mut cmd = Command::new(&script_path_owned);

            // Set environment variables
            for (key, value) in env_vars {
                cmd.env(key, value);
            }

            // Create dedicated log file for transcoding
            let log_file = output_dir_clone.join("transcode.log");

            tracing::debug!(cmd=?cmd, log_file=?log_file, "Invoking transcoding script.");
            match cmd.output() {
                Ok(res) => {
                    tracing::info!("Transcoding script finished");

                    // Write stdout/stderr to log file
                    let log_content = format!(
                        "=== Transcoding Script Execution ===\n\
                        Command: {:?}\n\
                        Exit Status: {}\n\
                        Media ID: {}\n\
                        User: {}\n\
                        Input: {:?}\n\
                        Output Dir: {:?}\n\
                        Timestamp: {}\n\n\
                        === STDOUT ===\n\
                        {}\n\n\
                        === STDERR ===\n\
                        {}\n\n",
                        &script_path_owned,
                        res.status,
                        &media_id,
                        &user_id,
                        &input_file,
                        &output_dir_clone,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        String::from_utf8_lossy(&res.stdout),
                        String::from_utf8_lossy(&res.stderr)
                    );

                    if let Err(e) = fs::write(&log_file, log_content) {
                        tracing::error!(file=?log_file, details=%e, "Failed to write transcoding log file");
                    }

                    (if res.status.success() {None} else {Some("Script exited with error".to_string())},
                        format!("Log written to: {}", log_file.display()),
                        "".to_string() )
                },
                Err(e) => {
                    tracing::error!(details=%e, "Script exec failed");

                    // Write error to log file
                    let log_content = format!(
                        "=== Transcoding Script Execution Failed ===\n\
                        Command: {:?}\n\
                        Error: {}\n\
                        Media ID: {}\n\
                        User: {}\n\
                        Input: {:?}\n\
                        Output Dir: {:?}\n\
                        Timestamp: {}\n\n",
                        &script_path_owned,
                        e,
                        &media_id,
                        &user_id,
                        &input_file,
                        &output_dir_clone,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );

                    if let Err(write_err) = fs::write(&log_file, log_content) {
                        tracing::error!(file=?log_file, details=%write_err, "Failed to write transcoding error log");
                    }

                    (Some(e.to_string()), format!("Log written to: {}", log_file.display()), "".into())
                }
            }
        })
    };

    // Wait for script to finish, then terminate progress thread
    tracing::debug!("Waiting for transcoding script to complete...");
    let (err_msg, stdout, stderr) = script_thread.join().unwrap_or_else(|e| {
        tracing::error!(details=?e, "Script thread panicked.");
        (Some("Script thread panicked".to_string()), "".into(), format!("{:?}", e))
    });

    tracing::debug!("Terminating script progress thread.");
    progress_terminate.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Err(e) = progress_thread.join() {
        tracing::warn!(details=?e, "Script progress thread panicked (ignoring).");
    }

    // Find the output file created by the script
    let video_dst = match err_msg {
        None => match find_script_output(&script_work_dir, &output_prefix) {
            Ok(script_output_path) => {
                // Move the output file from transcoded/ to the main media directory
                let filename = match script_output_path.file_name() {
                    Some(name) => name,
                    None => {
                        tracing::error!("Script output has invalid filename");
                        // Clean up progress pipe and sanitized symlink if they exist
                        if let Some(pipe_path) = &ppipe_fname {
                            if let Err(e) = fs::remove_file(pipe_path) {
                                tracing::debug!(details=%e, pipe=?pipe_path, "Failed to remove progress pipe (may not exist)");
                            }
                        }
                        if let Err(e) = fs::remove_file(&sanitized_input) {
                            tracing::debug!(details=%e, symlink=?sanitized_input, "Failed to remove sanitized symlink (may not exist)");
                        }
                        return CmprOutput::TranscodeFailure {
                            logs: CmprLogs {
                                media_file_id: src.media_file_id.clone(),
                                user_id: src.user_id.clone(),
                                stdout,
                                _stderr: stderr,
                                dmsg: DetailedMsg {
                                    msg: "Failed to get script output filename".to_string(),
                                    details: "Script output has invalid filename".to_string(),
                                    src_file: src.path.clone(),
                                    user_id: src.user_id.clone()
                                }
                            }
                        };
                    }
                };

                let final_output_path = output_dir.join(filename);

                if let Err(e) = fs::rename(&script_output_path, &final_output_path) {
                    tracing::error!(details=%e, from=?script_output_path, to=?final_output_path, "Failed to move script output to final location");
                    // Clean up progress pipe and sanitized symlink if they exist
                    if let Some(pipe_path) = &ppipe_fname {
                        if let Err(e) = fs::remove_file(pipe_path) {
                            tracing::debug!(details=%e, pipe=?pipe_path, "Failed to remove progress pipe (may not exist)");
                        }
                    }
                    if let Err(e) = fs::remove_file(&sanitized_input) {
                        tracing::debug!(details=%e, symlink=?sanitized_input, "Failed to remove sanitized symlink (may not exist)");
                    }
                    return CmprOutput::TranscodeFailure {
                        logs: CmprLogs {
                            media_file_id: src.media_file_id.clone(),
                            user_id: src.user_id.clone(),
                            stdout,
                            _stderr: stderr,
                            dmsg: DetailedMsg {
                                msg: "Failed to move script output to final location".to_string(),
                                details: format!("Error moving file: {}", e),
                                src_file: src.path.clone(),
                                user_id: src.user_id.clone()
                            }
                        }
                    };
                }

                tracing::debug!(from=?script_output_path, to=?final_output_path, "Moved script output to final location");
                final_output_path
            },
            Err(e) => {
                tracing::error!(details=%e, "Script completed but output validation failed");
                // Clean up progress pipe if it exists
                if let Some(pipe_path) = &ppipe_fname {
                    if let Err(e) = fs::remove_file(pipe_path) {
                        tracing::debug!(details=%e, pipe=?pipe_path, "Failed to remove progress pipe (may not exist)");
                    }
                }
                return CmprOutput::TranscodeFailure {
                    logs: CmprLogs {
                        media_file_id: src.media_file_id.clone(),
                        user_id: src.user_id.clone(),
                        stdout,
                        _stderr: stderr,
                        dmsg: DetailedMsg {
                            msg: "Script output validation failed".to_string(),
                            details: e,
                            src_file: src.path.clone(),
                            user_id: src.user_id.clone()
                        }
                    }
                };
            }
        },
        Some(_) => PathBuf::new() // Error case, path doesn't matter
    };

    let logs = CmprLogs {
        media_file_id: src.media_file_id.clone(),
        user_id: src.user_id.clone(),
        stdout,
        _stderr: stderr,
        dmsg: DetailedMsg {
            msg: if err_msg.is_some() { "Transcoding failed" } else { "Transcoding complete" }.to_string(),
            details: format!("Error in script: {:?}", err_msg.clone().unwrap_or_default()),
            src_file: src.path.clone(),
            user_id: src.user_id.clone()
        }
    };

    // Clean up progress pipe and sanitized symlink
    if let Some(pipe_path) = &ppipe_fname {
        if let Err(e) = fs::remove_file(pipe_path) {
            tracing::debug!(details=%e, pipe=?pipe_path, "Failed to remove progress pipe (may not exist)");
        }
    }

    if let Err(e) = fs::remove_file(&sanitized_input) {
        tracing::debug!(details=%e, symlink=?sanitized_input, "Failed to remove sanitized symlink (may not exist)");
    }

    // Attempt to clean up the script work directory
    if let Err(e) = fs::remove_dir_all(&script_work_dir) {
        tracing::warn!(details=%e, dir=?script_work_dir, "Failed to remove script work directory - may contain temporary files created by script");
    } else {
        tracing::debug!(dir=?script_work_dir, "Successfully cleaned up script work directory");
    }

    match err_msg {
        Some(_) => CmprOutput::TranscodeFailure { logs },
        None => CmprOutput::TranscodeSuccess { video_dst, logs }
    }
}

/// Run thumbnailing script
fn run_thumbnail_script(thumb_dir: PathBuf, thumb_size: (u32,u32), thumb_sheet_dims: (u32, u32),
                       src: CmprInputSource, script_path: &str) -> CmprOutput {
    let _span = tracing::info_span!("run_thumbnail_script",
        media_file = %src.media_file_id,
        user = %src.user_id,
        thread = ?std::thread::current().id()).entered();

    // Create sanitized symlink for script access
    let sanitized_input = match create_sanitized_symlink(&src.path) {
        Ok(path) => path,
        Err(e) => return err2cout("Failed to create sanitized symlink", e,
                                 &CmprInput::Thumbs {
                                     thumb_dir: thumb_dir.clone(),
                                     thumb_sheet_dims,
                                     thumb_size,
                                     src: src.clone()
                                 }, None)
    };

    // Create isolated script work directory for thumbnailing
    let script_work_dir = thumb_dir.join("transcoded");
    if let Err(e) = fs::create_dir_all(&script_work_dir) {
        return err2cout("Failed to create script work directory", e.to_string(),
                       &CmprInput::Thumbs {
                           thumb_dir: thumb_dir.clone(),
                           thumb_sheet_dims,
                           thumb_size,
                           src: src.clone()
                       }, Some(&sanitized_input));
    }

    // Set up environment variables for script
    let mut env_vars = HashMap::new();

    // Validate and set environment variables
    if let Err(e) = (|| -> Result<(), String> {
        env_vars.insert("CLAPSHOT_INPUT_FILE".to_string(),
                       validate_env_value("CLAPSHOT_INPUT_FILE", &sanitized_input.to_string_lossy())?);
        env_vars.insert("CLAPSHOT_OUTPUT_DIR".to_string(),
                       validate_env_value("CLAPSHOT_OUTPUT_DIR", &script_work_dir.to_string_lossy())?);
        env_vars.insert("CLAPSHOT_MEDIA_TYPE".to_string(),
                       validate_env_value("CLAPSHOT_MEDIA_TYPE", src.media_type.as_ref())?);
        env_vars.insert("CLAPSHOT_USER_ID".to_string(),
                       validate_env_value("CLAPSHOT_USER_ID", &src.user_id)?);
        env_vars.insert("CLAPSHOT_MEDIA_ID".to_string(),
                       validate_env_value("CLAPSHOT_MEDIA_ID", &src.media_file_id)?);
        env_vars.insert("CLAPSHOT_THUMB_SIZE".to_string(),
                       validate_env_value("CLAPSHOT_THUMB_SIZE", &format!("{}x{}", thumb_size.0, thumb_size.1))?);
        env_vars.insert("CLAPSHOT_SHEET_DIMS".to_string(),
                       validate_env_value("CLAPSHOT_SHEET_DIMS", &format!("{}x{}", thumb_sheet_dims.0, thumb_sheet_dims.1))?);
        Ok(())
    })() {
        return err2cout("Failed to set up script environment", e,
                       &CmprInput::Thumbs {
                           thumb_dir: thumb_dir.clone(),
                           thumb_sheet_dims,
                           thumb_size,
                           src: src.clone()
                       }, Some(&sanitized_input));
    }

    // Run the thumbnailing script
    let script_path_owned = script_path.to_string();
    let media_id = src.media_file_id.clone();
    let user_id = src.user_id.clone();
    let input_file = sanitized_input.clone();
    let thumb_dir_clone = thumb_dir.clone();
    let script_thread = {
        std::thread::spawn(move || {
            let _span = tracing::info_span!("thumbnail_script",
                thread = ?std::thread::current().id()).entered();

            let mut cmd = Command::new(&script_path_owned);

            // Set environment variables
            for (key, value) in env_vars {
                cmd.env(key, value);
            }

            // Create dedicated log file for thumbnailing
            let log_file = thumb_dir_clone.join("thumbnail.log");

            tracing::debug!(cmd=?cmd, log_file=?log_file, "Invoking thumbnailing script.");
            match cmd.output() {
                Ok(res) => {
                    tracing::info!("Thumbnailing script finished");

                    // Write stdout/stderr to log file
                    let log_content = format!(
                        "=== Thumbnailing Script Execution ===\n\
                        Command: {:?}\n\
                        Exit Status: {}\n\
                        Media ID: {}\n\
                        User: {}\n\
                        Input: {:?}\n\
                        Thumb Dir: {:?}\n\
                        Timestamp: {}\n\n\
                        === STDOUT ===\n\
                        {}\n\n\
                        === STDERR ===\n\
                        {}\n\n",
                        &script_path_owned,
                        res.status,
                        &media_id,
                        &user_id,
                        &input_file,
                        &thumb_dir_clone,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        String::from_utf8_lossy(&res.stdout),
                        String::from_utf8_lossy(&res.stderr)
                    );

                    if let Err(e) = fs::write(&log_file, log_content) {
                        tracing::error!(file=?log_file, details=%e, "Failed to write thumbnailing log file");
                    }

                    (if res.status.success() {None} else {Some("Script exited with error".to_string())},
                        format!("Log written to: {}", log_file.display()),
                        "".to_string() )
                },
                Err(e) => {
                    tracing::error!(details=%e, "Script exec failed");

                    // Write error to log file
                    let log_content = format!(
                        "=== Thumbnailing Script Execution Failed ===\n\
                        Command: {:?}\n\
                        Error: {}\n\
                        Media ID: {}\n\
                        User: {}\n\
                        Input: {:?}\n\
                        Thumb Dir: {:?}\n\
                        Timestamp: {}\n\n",
                        &script_path_owned,
                        e,
                        &media_id,
                        &user_id,
                        &input_file,
                        &thumb_dir_clone,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );

                    if let Err(write_err) = fs::write(&log_file, log_content) {
                        tracing::error!(file=?log_file, details=%write_err, "Failed to write thumbnailing error log");
                    }

                    (Some(e.to_string()), format!("Log written to: {}", log_file.display()), "".into())
                }
            }
        })
    };

    // Wait for script to finish
    tracing::debug!("Waiting for thumbnailing script to complete...");
    let (err_msg, stdout, stderr) = script_thread.join().unwrap_or_else(|e| {
        tracing::error!(details=?e, "Script thread panicked.");
        (Some("Script thread panicked".to_string()), "".into(), format!("{:?}", e))
    });

    let logs = CmprLogs {
        media_file_id: src.media_file_id.clone(),
        user_id: src.user_id.clone(),
        stdout,
        _stderr: stderr,
        dmsg: DetailedMsg {
            msg: if err_msg.is_some() { "Thumbnailing failed" } else { "Thumbnailing complete" }.to_string(),
            details: format!("Error in script: {:?}", err_msg.clone().unwrap_or_default()),
            src_file: src.path.clone(),
            user_id: src.user_id.clone()
        }
    };

    // Move thumbnail files from script work directory to main thumbnail directory
    // and clean up the isolated work directory
    if err_msg.is_none() {
        // Copy thumbnail files from transcoded/ to main directory
        if let Ok(entries) = fs::read_dir(&script_work_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let src_path = entry.path();
                if src_path.is_file() {
                    let filename = src_path.file_name().unwrap();
                    let dest_path = thumb_dir.join(filename);

                    if let Err(e) = fs::rename(&src_path, &dest_path) {
                        tracing::warn!(details=%e, from=?src_path, to=?dest_path, "Failed to move thumbnail file to final location");
                        // Try copy + remove as fallback
                        if let Ok(()) = fs::copy(&src_path, &dest_path).and_then(|_| fs::remove_file(&src_path)) {
                            tracing::debug!(from=?src_path, to=?dest_path, "Successfully copied and removed thumbnail file");
                        }
                    } else {
                        tracing::debug!(from=?src_path, to=?dest_path, "Moved thumbnail file to final location");
                    }
                }
            }
        }
    }

    // Clean up sanitized symlink
    if let Err(e) = fs::remove_file(&sanitized_input) {
        tracing::debug!(details=%e, symlink=?sanitized_input, "Failed to remove sanitized symlink (may not exist)");
    }

    // Attempt to clean up the script work directory
    if let Err(e) = fs::remove_dir_all(&script_work_dir) {
        tracing::warn!(details=%e, dir=?script_work_dir, "Failed to remove script work directory - may contain temporary files created by script");
    } else {
        tracing::debug!(dir=?script_work_dir, "Successfully cleaned up script work directory");
    }

    match err_msg {
        Some(_) => CmprOutput::ThumbsFailure { logs },
        None => {
            // Check if any thumbnail files were actually created
            let has_thumbnails = thumb_dir.exists() &&
                fs::read_dir(&thumb_dir)
                    .map(|entries| entries.filter_map(|e| e.ok()).any(|entry| {
                        let path = entry.path();
                        let is_thumb = path.is_file() && path.file_name()
                            .and_then(|name| name.to_str())
                            .map(|s| {
                                // Only consider actual thumbnail files, not log files
                                s.ends_with(".webp") || 
                                (s.starts_with("thumb") && s.ends_with(".webp")) || 
                                (s.starts_with("sheet-") && s.ends_with(".webp"))
                            })
                            .unwrap_or(false);
                        is_thumb
                    }))
                    .unwrap_or(false);
            

            CmprOutput::ThumbsSuccess {
                thumb_dir: if has_thumbnails { Some(thumb_dir) } else { None },
                thumb_sheet_dims: if has_thumbnails { Some(thumb_sheet_dims) } else { None },
                logs
            }
        }
    }
}

/// Listen to incoming transcoding/thumbnailing requests and spawn script processes
pub fn run_forever(
    inq: Receiver<CmprInput>,
    outq: Sender<CmprOutput>,
    progress: ProgressSender,
    n_workers: usize,
    transcode_script: String,
    thumbnail_script: String)
{
    let _span = tracing::info_span!("SCRIPT_PROCESSOR").entered();
    tracing::debug!(n_workers = n_workers, "Starting script processor.");

    let pool = ThreadPool::new(n_workers);
    loop {
        match inq.recv() {
            Ok(args) => {
                match &args {
                    CmprInput::Transcode { src, .. } => {
                        tracing::info!(id=%src.media_file_id, r#type=?src.media_type,
                            user=%src.user_id, file=%(src.path.file_name().unwrap_or_default().to_string_lossy()),
                            "Media file transcode request (script).");
                    },
                    CmprInput::Thumbs { src, .. } => {
                        tracing::info!(id=%src.media_file_id, r#type=?src.media_type,
                            user=%src.user_id, file=%(src.path.file_name().unwrap_or_default().to_string_lossy()),
                            "Media file thumbnail request (script).");
                    },
                }
                tracing::debug!(details=?args, "Spawning script worker thread.");

                let outq = outq.clone();
                let prgr_sender = progress.clone();
                let transcode_script_path = transcode_script.clone();
                let thumbnail_script_path = thumbnail_script.clone();

                pool.execute(move || {
                    match args {
                        CmprInput::Transcode { video_dst_dir, video_dst_prefix, video_bitrate, src } => {
                            if let Err(e) = outq.send(run_transcode_script(&src, video_dst_dir, video_dst_prefix, video_bitrate, prgr_sender, &transcode_script_path)) {
                                tracing::error!("Transcode result send failed! Aborting. -- {:?}", e);
                            }
                        },
                        CmprInput::Thumbs { thumb_dir, thumb_sheet_dims, thumb_size, src } => {
                            if let Err(e) = outq.send(run_thumbnail_script(thumb_dir, thumb_size, thumb_sheet_dims, src, &thumbnail_script_path)) {
                                tracing::error!("Thumbnail result send failed! Aborting. -- {:?}", e);
                            }
                        },
                    }
                });
            },
            Err(e) => {
                tracing::info!(details=%e, "Input queue closed.");
                break;
            }
        }
    }

    tracing::debug!("Exiting script processor.");
}