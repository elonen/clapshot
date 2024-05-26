use std::{process::Command, io::BufRead};
use std::path::PathBuf;
use crossbeam_channel::{Sender, Receiver};
use rust_decimal::Decimal;
use tracing;
use threadpool::ThreadPool;

use super::metadata_reader::MediaType;
use super::DetailedMsg;

pub type ProgressSender = crossbeam_channel::Sender<(String, String, String)>;


// Input to the FFMPEG processor

#[derive(Debug, Clone)]
pub enum CmprInput {
    Transcode {
        video_dst: PathBuf,
        video_bitrate: u32,
        src: CmprInputSource,
    },
    Thumbs {
        thumb_dir: PathBuf,
        thumb_sheet_dims: (u32, u32),   // cols, rows: how many thumbnails in the sheet
        thumb_size: (u32, u32),         // width, height: resolution of a single thumbnail
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


// Output from the FFMPEG processor

#[derive(Debug, Clone)]
pub enum CmprOutput {
    TranscodeSuccess {
        video_dst: PathBuf,
        logs: CmprLogs
    },
    ThumbsSuccess {
        thumb_dir: Option<PathBuf>,
        thumb_sheet_dims: Option<(u32, u32)>,   // cols, rows
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
    pub stderr: String,
    pub dmsg: DetailedMsg,
}



fn err2cout<E: std::fmt::Debug>(msg_txt: &str, err: E, args: &CmprInput) -> CmprOutput {
    let details_str = format!("{:?}", err);
    tracing::error!(details=&details_str, "err2cout: {}", msg_txt);

    let src = match args {
        CmprInput::Transcode { src, .. } | CmprInput::Thumbs { src, .. } => src,
    };

    let logs = CmprLogs {
        media_file_id: src.media_file_id.clone(),
        user_id: src.user_id.clone(),
        stdout: "".into(),
        stderr: "".into(),
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

/// Run FFMpeg shell command and return the output (stdout, stderr)
/// Send progress updates to the progress channel.
///
/// # Arguments
/// * `args` - what to compress and where to put the result
/// * `progress` - channel to send progress updates to
///
fn run_ffmpeg_transcode(src: &CmprInputSource, video_dst: PathBuf, video_bitrate: u32, progress: ProgressSender ) -> CmprOutput
{
    let _span = tracing::info_span!("run_ffmpeg_transcode",
        media_file = %src.media_file_id,
        user = %src.user_id,
        thread = ?std::thread::current().id()).entered();

    // Construct ffmpeg options based on media type
    let bitrate = video_bitrate.to_string();
    let duration = src.duration.to_string();
    let audio_filter_complex = format!(
        "color=c=white:s=2x720 [cursor]; \
        [0:a] showwavespic=s=1920x720:split_channels=1:draw=full, fps=60 [stillwave];\
        [0:a] showfreqs=mode=line:ascale=log:s=1920x180 [freqwave]; \
        [0:a] showwaves=size=1920x180:mode=p2p [livewave]; \
        [stillwave][cursor] overlay=(W*t)/({}):0:shortest=1 [progress]; \
        [livewave][progress] vstack[stacked]; \
        [stacked][freqwave] vstack [out];", &duration);

    let ffmpeg_options: Vec<String> = match src.media_type {
        MediaType::Video => {
            // Max 1080p, stereo 128kbps audio AAC,
            vec![
                "-map", "0",
                "-dn",
                "-vcodec", "libx264",
                "-vf", "scale=1920:-8",
                "-preset", "faster",
                "-acodec", "aac",
                "-ac", "2",
                "-strict", "experimental",
                "-b:v", &bitrate,
                "-b:a", "128000"
            ]
        },
        MediaType::Audio => {
            vec![
                "-dn",
                "-r", "60",
                "-filter_complex", &audio_filter_complex,
                "-map", "[out]",
                "-map", "0:a",
                "-strict", "experimental",
                "-vcodec", "libx264",
                "-b:v", &bitrate,
                "-acodec", "flac"
            ]
        },
        MediaType::Image => {
            vec![
                "-map", "0",
                "-dn",
                "-vcodec", "libx264",
                "-vf", "scale=1920:-8",
                "-framerate", "1",
                "-r", "30",
                "-pix_fmt", "yuv444p",
                "-b:v", &bitrate,
                "-b:a", "128000"
            ]
        }
    }.iter().map(|s| s.to_string()).collect();


    tracing::info!(bitrate=video_bitrate, "Transcoder called.");
    tracing::debug!(ffmpeg_options=?ffmpeg_options, "FFMPEG options");



    // Open a named pipe for ffmpeg to write progress reports to.
    // If this fails, ignore it and just don't show progress.
    let ppipe_fname = {
        let fname = video_dst.with_extension("progress").with_extension("pipe");
        match fname.to_str() {
            None => { Err("Invalid dst path".to_string()) }
            Some(fname) => unix_named_pipe::create(&fname, None)
                .map(|_| fname.to_string())
                .map_err(|e| e.to_string())
        }.map_or_else(|e| { tracing::warn!(details=e, "Won't track FFMPEG progress; failed to create pipe file."); None}, |f| Some(f))
    };

    // Start encoder thread
    let ffmpeg_thread = {
        let src = src.path.clone();
        let dst = video_dst.clone();
        let ppipe_fname = ppipe_fname.clone();

        std::thread::spawn(move || {
            let _span = tracing::info_span!("ffmpeg_transcode",
                thread = ?std::thread::current().id()).entered();

            let mut cmd = &mut Command::new("nice");
            cmd = cmd.args(&["ffmpeg", "-nostats", "-hide_banner", "-y", "-i"]).arg(&src);

            // Add proggress reporting
            if let Some(pfn) = ppipe_fname { cmd = cmd.args(&["-progress", &pfn]); }

            // Add media type specific options
            cmd = cmd.args(&ffmpeg_options).arg(&dst);

            tracing::debug!(cmd=?cmd, "Invoking ffmpeg.");
            match cmd.output() {
                Ok(res) => {
                    tracing::info!("ffmpeg finished");
                    (if res.status.success() {None} else {Some("FFMPEG exited with error".to_string())},
                        String::from_utf8_lossy(&res.stdout).to_string(),
                        String::from_utf8_lossy(&res.stderr).to_string() )
                },
                Err(e) => {
                    tracing::error!(details=%e, "ffmpeg exec failed");
                    (Some(e.to_string()), "".into(), "".into())
                }
            }
        })};

    let progress_terminate = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Thread to read FFMPEG progress report from named pipe and send updates to user(s)
    let progress_thread =
    {
        let progress_terminate = progress_terminate.clone();
        let user_id = src.user_id.clone();
        match ppipe_fname {
            None => std::thread::spawn(move || {}), // No named pipe, skip progress tracking
            Some(pfn) => {
                let vid = src.media_file_id.clone();
                let src = src.path.clone();
                std::thread::spawn(move || {
                    let _span = tracing::info_span!("ffmpeg_progress",
                        thread = ?std::thread::current().id()).entered();

                    let frame_count = count_video_frames(&src);

                    let f = match unix_named_pipe::open_read(&pfn) {
                        Ok(f) => f,
                        Err(e) => {
                            tracing::error!(details=%e, "Failed to open named pipe.");
                            return;
                        }
                    };
                    let reader = &mut std::io::BufReader::new(&f);

                    let mut msg : Option<String> = None;
                    let mut frame_i = Option::<i32>::None;
                    let mut fps = -1f32;

                    while !progress_terminate.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        // Read progress lines from pipe & parse
                        match reader.lines().next() {
                            Some(Err(e)) => {
                                // Pipe is non-blocking, so ignore EAGAIN, handle others
                                if e.kind() != std::io::ErrorKind::WouldBlock {
                                    tracing::error!(details=%e, "Failed to read from pipe.");
                                    break;
                                } else {
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                }
                        },
                            None => { tracing::debug!("Progress pipe EOF"); break; }
                            Some(Ok(l)) => {
                                // FFMPEG Progress "chunk" looks like this:
                                //    frame=584
                                //    fps=52.40
                                //       <...some other key-value pairs...>
                                //    progress=continue
                                match l.find("=") {
                                    None => { tracing::debug!(input=l, "Skipping invalid FFMPEG progress chunk line."); }
                                    Some(idx) => {
                                        let (key, val) = l.split_at(idx);
                                        let val = &val[1..];
                                        match key {
                                            "frame" => {
                                                match val.parse::<i32>() {
                                                    Ok(n) => { frame_i = Some(n); },
                                                    Err(e) => { tracing::warn!(details=%e, "Invalid frame# in FFMPEG progress log."); }
                                                }},
                                            "fps" => {
                                                match val.parse::<f32>() {
                                                    Ok(n) => { fps = n; },
                                                    Err(e) => { tracing::warn!(details=%e, "Invalid fps in FFMPEG progress log."); }
                                                }},
                                            "progress" => {
                                                match val {
                                                    "end" => { msg = Some("Transcoding done.".to_string()); },
                                                    _ => {
                                                        let fps_str = if fps > 0f32 { format!(" (speed: {:.1} fps)", fps) } else { "".to_string() };
                                                        match (frame_i, frame_count) {
                                                            (Some(frame), Some(n_frames)) => {
                                                                let ratio = frame as f32 / n_frames as f32;
                                                                msg = Some(format!("Transcoding... {:.1}% done{fps_str}", (ratio * 100f32) as i32));
                                                            },
                                                            _ => { msg = Some(format!("Transcoding...{fps_str}")); }
                                                        }}}},
                                            _ => {}, // Ignore other keys
                                        }}}
                                // Send progress message (if any)
                                if let Some(msg) = msg.take() {
                                    if let Err(e) = progress.send((vid.clone(), user_id.clone(), msg)) {
                                        tracing::debug!(details=%e, "Failed to send FFMPEG progress message. Ending progress tracking.");
                                        return;
                                    }}
                            }}}})
                        }
                    }
    };

    // Wait for FFMPEG to finish, then terminate progress thread
    let (err_msg, stdout, stderr) = ffmpeg_thread.join().unwrap_or_else(|e| {
        tracing::error!(details=?e, "FFMPEG thread panicked.");
        (Some("FFMPEG thread panicked".to_string()), "".into(), format!("{:?}", e))
    });
    tracing::debug!("FFMPEG encoder thread joined.");

    progress_terminate.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Err(e) = progress_thread.join() {
        tracing::warn!(details=?e, "FFMPEG progress reporter thread panicked (ignoring).");
    }
    tracing::debug!("FFMPEG progress thread joined.");

    let logs = CmprLogs {
        media_file_id: src.media_file_id.clone(),
        user_id: src.user_id.clone(),
        stdout: stdout,
        stderr: stderr,
        dmsg: DetailedMsg {
            msg: if err_msg.is_some() { "Transcoding failed" } else { "Transcoding complete" }.to_string(),
            details: format!("Error in FFMPEG: {:?}", err_msg.clone().unwrap_or_default()),
            src_file: src.path.clone(),
            user_id: src.user_id.clone()
        }
    };
    match err_msg {
        Some(_) => CmprOutput::TranscodeFailure { logs },
        None => CmprOutput::TranscodeSuccess { video_dst, logs }
    }
}


/// Use ffprobe to find how many frames are in the video
///
/// # Arguments
/// * `file_path` - Path to the file to be analyzed
/// # Returns
/// * Number of frames in the video
fn count_video_frames( src: &PathBuf ) -> Option<u32>
{
    match Command::new("ffprobe").args(&["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0"]).arg(&src).output() {
        Ok(output) => {
            if output.status.success() {
                match String::from_utf8_lossy(&output.stdout).trim().parse::<usize>() {
                    Ok(n) => { return Some(n as u32); },
                    Err(e) => { tracing::error!(details=%e, file=?src, "Frame counting failed: invalid u32 parse"); }
                }
            } else { tracing::error!(details=%String::from_utf8_lossy(&output.stderr), file=?src, "Frame counting failed; ffprobe exited with error."); }
        },
        Err(e) => { tracing::error!(details=%e, file=?src, "Ffprobe exec failed."); }
    };
    None
}


/// Extract exactly THUMB_COUNT frames (THUMB_W x THUMB_H, letterboxed) that cover the whole media file
/// and save them as WEBP files (thumb_NN.webp) in the given directory.
/// Copy the first frame also as thumb.webp (for fast preview without seeking).
///
/// # Arguments
/// * `args` - what to process and where to put the result
///
fn run_ffmpeg_thumbnailer( thumb_dir: PathBuf, thumb_size: (u32,u32), thumb_sheet_dims: (u32, u32), src: CmprInputSource ) -> CmprOutput
{
    let _span = tracing::info_span!("run_ffmpeg_thumbnailer",
        media_file = %src.media_file_id,
        media_type = ?src.media_type,
        user = %src.user_id,
        thread = ?std::thread::current().id()).entered();

    // Determine if we need to create a "poster" thumbnail (single frame) and/or a thumbnail sheet
    let (needs_poster, needs_sheet) = match &src.media_type {
        MediaType::Video => (true, true),
        MediaType::Image => (true, false),
        MediaType::Audio => (false, false),
    };

    tracing::info!(poster=needs_poster, sheet=needs_sheet, "Thumbnailer called.");

    if !thumb_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&thumb_dir) {
            return err2cout("Failed to create thumbnail directory", &e.to_string(), &CmprInput::Thumbs { thumb_dir, thumb_size, thumb_sheet_dims, src });
        }
    }

    let (thumb_w, thumb_h) = thumb_size;

    // Create "poster" thumbnail (probably first frame, but ffmpeg can choose any)
    let single_thumb_thread = {
        let src_path = src.path.clone();
        let thumb_dir = thumb_dir.clone();
        std::thread::spawn(move || {
            let _span = tracing::info_span!("ffmpeg_thumb_poster",
                thread = ?std::thread::current().id()).entered();

            if !needs_poster {
                tracing::debug!("Skipping poster thumbnail creation: not needed for this media type.");
                return (None, "".into(), "".into());
            }

            let img_reshape = format!("scale={thumb_w}:{thumb_h}:force_original_aspect_ratio=decrease,pad={thumb_w}:{thumb_h}:(ow-iw)/2:(oh-ih)/2");

            let mut cmd = &mut Command::new("nice");
            cmd = cmd.arg("-n").arg("10").arg("--")
                .arg("ffmpeg").arg("-y").arg("-i").arg(&src_path).args(&[
                "-nostats",
                "-vcodec", "libwebp",
                "-vf", format!("thumbnail,{img_reshape}",).as_str(),
                "-frames:v", "1",
                "-strict", "experimental",
                "-c:v", "libwebp",
            ]).arg(thumb_dir.join("thumb.webp"));

            tracing::debug!(cmd=?cmd, "Invoking ffmpeg.");
            match cmd.output() {
                Ok(res) => {
                    tracing::info!("ffmpeg finished");
                    (if res.status.success() {None} else {Some("FFMPEG exited with error".to_string())},
                        String::from_utf8_lossy(&res.stdout).to_string(),
                        String::from_utf8_lossy(&res.stderr).to_string() )
                },
                Err(e) => {
                    tracing::error!(details=%e, "ffmpeg exec failed");
                    (Some(e.to_string()), "".into(), "".into())
                }
            }
        }
    )};

    // Create thumbnail sheet (preview of the whole media file)
    let sheet_thread = {
        let src_path = src.path.clone();
        let thumb_dir = thumb_dir.clone();
        std::thread::spawn(move || {
            let _span = tracing::info_span!("ffmpeg_thumbsheet",
                thread = ?std::thread::current().id()).entered();

            if !needs_sheet {
                tracing::debug!("Skipping thumb sheet creation: not needed for this media type.");
                return (None, "".into(), "".into());
            }
            let (thumb_sheet_cols, thumb_sheet_rows) = thumb_sheet_dims;
            let thumb_count = thumb_sheet_cols * thumb_sheet_rows;
            assert!(thumb_count > 0);

            let img_reshape = format!("scale={thumb_w}:{thumb_h}:force_original_aspect_ratio=decrease,pad={thumb_w}:{thumb_h}:(ow-iw)/2:(oh-ih)/2");

            let total_frames = match count_video_frames(&src_path) {
                Some(d) => d,
                None => return (Some("ffprobe count_frames failed".to_string()), "".into(), "".into())
            };

            // Make a "-vf" filter that selects exactly THUMB_COUNT frames from the video
            let frame_select_filter = (0..thumb_count).map(|pos| {
                    let frame = pos * total_frames / thumb_count;
                    format!("eq(n\\,{})", frame)
                }).collect::<Vec<String>>().join("+");

            let mut cmd = &mut Command::new("nice");
            cmd = cmd.arg("-n").arg("10").arg("--")
                .arg("ffmpeg").arg("-y").arg("-i").arg(&src_path).args(&[
                "-nostats",
                "-vf", &format!("select={frame_select_filter},{img_reshape},tile={thumb_sheet_cols}x{thumb_sheet_rows}"),
                "-strict", "experimental",
                "-c:v", "libwebp",
                "-vsync", "vfr",
                "-start_number", "0",
            ]).arg(thumb_dir.join(format!("sheet-{thumb_sheet_cols}x{thumb_sheet_rows}.webp")));

            tracing::debug!(cmd=?cmd, "Invoking ffmpeg.");
            match cmd.output() {
                Ok(res) => {
                    tracing::info!("ffmpeg finished");
                    (if res.status.success() {None} else {Some("FFMPEG exited with error".to_string())},
                        String::from_utf8_lossy(&res.stdout).to_string(),
                        String::from_utf8_lossy(&res.stderr).to_string() )
                },
                Err(e) => {
                    tracing::error!(details=%e, "ffmpeg exec failed");
                    (Some(e.to_string()), "".into(), "".into())
                }
            }
        }
    )};

    // Wait for processes to finish
    let mut comb_err = None;
    let mut comb_stdout = String::new();
    let mut comb_stderr = String::new();
    for (name, thread) in vec![("poster", single_thumb_thread), ("sheet", sheet_thread)].into_iter() {
        let (err, stdout, stderr) = match thread.join() {
            Ok(res) => {
                tracing::debug!("Thread '{name}' finished");
                res
            },
            Err(e) => {
                tracing::error!(details=?e, "FFMPEG thumbnailer '{name}' thread panicked.");
                (Some(format!("Thread '{name}' panicked", name=name)), "".into(), format!("{:?}", e))
            }
        };
        match (&comb_err, err) {
            (None, Some(msg)) => { comb_err = Some(msg) },
            (Some(prev_msg), Some(msg)) => { comb_err = Some(format!("{} ; {}", prev_msg, msg)) },
            _ => ()
        };
        comb_stdout.push_str(format!("--- {name} ---\n{stdout}\n\n").as_str());
        comb_stderr.push_str(format!("--- {name} ---\n{stderr}\n\n").as_str());
    };

    let logs = CmprLogs {
        media_file_id: src.media_file_id.clone(),
        user_id: src.user_id.clone(),
        stdout: comb_stdout,
        stderr: comb_stderr,
        dmsg: DetailedMsg {
            msg: if comb_err.is_some() { "Thumbnailing failed" } else { "Thumbnailing complete" }.to_string(),
            details: format!("Error in FFMPEG: {}", comb_err.clone().unwrap_or_default()),
            src_file: src.path.clone(),
            user_id: src.user_id.clone()
        }
    };
    match comb_err {
        Some(_) => CmprOutput::ThumbsFailure { logs },
        None => CmprOutput::ThumbsSuccess {
            thumb_dir: if needs_poster || needs_sheet {Some(thumb_dir)} else {None},
            thumb_sheet_dims: if needs_sheet {Some(thumb_sheet_dims)} else {None},
            logs
        }
    }
}


/// Listen to incoming transcoding/thumbnailing requests and spawn a thread (from a pool) to handle each one.
/// Calls FFMpeg CLI to do the actual work, and sends progress updates to the given channel.
///
/// # Arguments
/// * `inq` - Channel to receive incoming requests
/// * `outq` - Channel to send results
/// * `progress` - Channel to send transcoding progress updates. Tuple: (media_file_id, progress_msg)
/// * `n_workers` - Number of worker threads to spawn for processing. This should be at most the number of CPU cores.
pub fn run_forever(
    inq: Receiver<CmprInput>,
    outq: Sender<CmprOutput>,
    progress: ProgressSender,
    n_workers: usize)
{
    let _span = tracing::info_span!("COMPR").entered();
    tracing::debug!(n_workers = n_workers, "Starting.");

    let pool = ThreadPool::new(n_workers);
    loop {
        match inq.recv() {
            Ok(args) => {
                //tracing::info!("Got message: {:?}", args);
                match &args {
                    CmprInput::Transcode { src, .. } => {
                        tracing::info!(id=%src.media_file_id, r#type=?src.media_type,
                            user=%src.user_id, file=%(src.path.file_name().unwrap_or_default().to_string_lossy()),
                            "Media file transcode request.");
                    },
                    CmprInput::Thumbs { src, .. } => {
                        tracing::info!(id=%src.media_file_id, r#type=?src.media_type,
                            user=%src.user_id, file=%(src.path.file_name().unwrap_or_default().to_string_lossy()),
                            "Media file thumbnail request.");
                    },
                }
                tracing::debug!(details=?args, "Spawning worker thread.");

                let outq = outq.clone();
                let prgr_sender = progress.clone();
                pool.execute(move || {
                    match args {
                        CmprInput::Transcode { video_dst, video_bitrate, src } => {
                            if let Err(e) = outq.send(run_ffmpeg_transcode(&src, video_dst, video_bitrate, prgr_sender)) {
                                tracing::error!("Transcode result send failed! Aborting. -- {:?}", e);
                            }
                        },
                        CmprInput::Thumbs { thumb_dir, thumb_sheet_dims, thumb_size, src } => {
                            if let Err(e) = outq.send(run_ffmpeg_thumbnailer(thumb_dir, thumb_size, thumb_sheet_dims, src)) {
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

    tracing::debug!("Exiting.");
}
