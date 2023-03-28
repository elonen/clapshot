use std::{process::Command, io::BufRead};
use std::path::{PathBuf};
use crossbeam_channel::{Sender, Receiver};
use tracing;
use threadpool::ThreadPool;

use super::DetailedMsg;

pub type ProgressSender = crossbeam_channel::Sender<(String, String, String)>;

use super::{THUMB_SHEET_COLS, THUMB_SHEET_ROWS, THUMB_W, THUMB_H};
const THUMB_COUNT: u32 = THUMB_SHEET_COLS * THUMB_SHEET_ROWS;


#[derive(Debug, Clone)]
pub struct CmprInput {
    pub src: PathBuf,
    pub video_dst: Option<PathBuf>,
    pub thumb_dir: Option<PathBuf>,
    pub video_bitrate: u32,
    pub video_hash: String,
    pub user_id: String,
}

#[derive(Debug, Clone)]
pub struct CmprOutput {
    pub success: bool,
    pub video_dst: Option<PathBuf>,
    pub thumb_dir: Option<PathBuf>,
    pub video_hash: String,
    pub stdout: String,
    pub stderr: String,
    pub dmsg: DetailedMsg,
    pub user_id: String,
}

fn err2cout<E: std::fmt::Debug>(msg_txt: &str, err: E, args: &CmprInput) -> CmprOutput {
    let details_str = format!("{:?}", err);
    tracing::error!(details=&details_str, "err2cout: {}", msg_txt);

    CmprOutput {
        success: false,
        video_dst: args.video_dst.clone(),
        thumb_dir: args.thumb_dir.clone(),
        video_hash: args.video_hash.clone(),
        stdout: "".into(),
        stderr: "".into(),
        dmsg: DetailedMsg {
            msg: msg_txt.to_string(),
            details: details_str,
            src_file: args.src.clone(),
            user_id: args.user_id.clone()
        },
        user_id: args.user_id.clone()
    }
}

/// Run FFMpeg shell command and return the output (stdout, stderr)
/// Send progress updates to the progress channel.
/// 
/// # Arguments
/// * `args` - what to compress and where to put the result
/// * `progress` - channel to send progress updates to
/// 
fn run_ffmpeg_transcode( args: CmprInput, progress: ProgressSender ) -> CmprOutput
{
    let _span = tracing::info_span!("run_ffmpeg_transcode",
        video = %args.video_hash,
        user = %args.user_id,
        thread = ?std::thread::current().id()).entered();

    let video_dst = match args.video_dst.clone() {
        Some(p) => p,
        None => return err2cout("BUG: transcode called with no video destination", "", &args)
    };

    tracing::info!(src=%args.src.display(), dst=%video_dst.display(), bitrate=%args.video_bitrate, "Compressing video");

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
        let src = args.src.clone();
        let dst = video_dst.clone();
        let ppipe_fname = ppipe_fname.clone();        
        std::thread::spawn(move || {
            let _span = tracing::info_span!("ffmpeg_transcode_thread",
                thread = ?std::thread::current().id()).entered();
    
            let mut cmd = &mut Command::new("nice");
            cmd = cmd.arg("-n").arg("10").arg("--")
                .arg("ffmpeg").arg("-i").arg(&src);

            if let Some(pfn) = ppipe_fname {
                cmd = cmd.args(&["-progress", &pfn]);
            }
            cmd = cmd.args(&[
                "-nostats",
                "-vcodec", "libx264",
                "-vf", &format!("scale={}:{}", 1920, -8),
                "-map", "0",  // copy all streams
                "-preset", "faster",
                "-acodec", "aac",
                "-ac", "2",
                "-strict", "experimental",
                "-b:v", &format!("{}", args.video_bitrate),
                "-b:a", &format!("{}", 128000),
            ]).arg(&dst);

            tracing::info!("Calling ffmpeg");
            tracing::debug!("Exec: {:?}", cmd);
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
        let user_id = args.user_id.clone();
        match ppipe_fname {
            None => std::thread::spawn(move || {}),
            Some(pfn) => {
                let vh = args.video_hash.clone();
                let src = args.src.clone();
                std::thread::spawn(move || {
                    let _span = tracing::info_span!("progress_thread",
                        thread = ?std::thread::current().id()).entered();
    
                    let total_frames = count_frames(&src);

                    let f = match unix_named_pipe::open_read(&pfn) {
                        Ok(f) => f,
                        Err(e) => {
                            tracing::error!(details=%e, "Failed to open named pipe.");
                            return;
                        }
                    };
                    let reader = &mut std::io::BufReader::new(&f);

                    let mut msg : Option<String> = None;
                    let mut frame = Option::<i32>::None;
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
                                                    Ok(n) => { frame = Some(n); },
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
                                                        match (frame, total_frames) {
                                                            (Some(frame), Some(total_frames)) => {
                                                                let ratio = frame as f32 / total_frames as f32;
                                                                msg = Some(format!("Transcoding... {:.1}% done{fps_str}", (ratio * 100f32) as i32));
                                                            },
                                                            _ => { msg = Some(format!("Transcoding...{fps_str}")); }
                                                        }}}},
                                            _ => {}, // Ignore other keys
                                        }}}
                                // Send progress message (if any)
                                if let Some(msg) = msg.take() {
                                    if let Err(e) = progress.send((vh.clone(), user_id.clone(), msg)) {
                                        tracing::debug!(details=%e, "Failed to send FFMPEG progress message. Ending progress tracking.");
                                        return;
                                    }}
                            }}}})
                        }
                    }
    };

    // Wait for FFMPEG to finish, then terminate progress thread
    let (err_msg, stdout, stderr) = match ffmpeg_thread.join() {
        Ok(res) => res,
        Err(e) => {
            tracing::error!(details=?e, "FFMPEG thread panicked.");
            (Some("FFMPEG thread panicked".to_string()), "".into(), format!("{:?}", e))
        }
    };
    tracing::debug!("FFMPEG encoder thread joined.");
    progress_terminate.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Err(e) = progress_thread.join() {
        tracing::warn!(details=?e, "FFMPEG progress reporter thread panicked (ignoring).");
    }
    tracing::debug!("FFMPEG progress thread joined.");

    CmprOutput {
        success: err_msg.is_none(),
        video_dst: Some(video_dst),
        thumb_dir: None,
        video_hash: args.video_hash.clone(),
        stdout: stdout,
        stderr: stderr,
        dmsg: DetailedMsg {
            msg: if err_msg.is_some() { "Transcoding failed" } else { "Transcoding complete" }.to_string(),
            details: format!("Error in FFMPEG: {:?}", err_msg),
            src_file: args.src.clone(),
            user_id: args.user_id.clone()
        },
        user_id: args.user_id.clone(),
    }
}

/// Use ffprobe to find how many frames are in the video
///
/// # Arguments
/// * `file_path` - Path to the file to be analyzed
/// # Returns
/// * Number of frames in the video
fn count_frames( src: &PathBuf ) -> Option<i32>
{
    // Equiv to: ffprobe -v error -select_streams v:0 -count_packets -show_entries stream=nb_read_packets -of csv=p=0 <INPUT-FILE>
    let cmd_res = Command::new("ffprobe")
        .args(&["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0"])
        .arg(&src).output();
    match cmd_res {
        Ok(output) => {
            if output.status.success() {
                match String::from_utf8_lossy(&output.stdout).trim().parse::<u32>() {
                    Ok(n) => { return Some(n as i32); },
                    Err(e) => { tracing::error!(details=%e, file=?src, "Frame counting failed: invalid u32 parse"); }
                }
            } else { tracing::error!(details=%String::from_utf8_lossy(&output.stderr), file=?src, "Frame counting failed; ffprobe exited with error."); }
        },
        Err(e) => { tracing::error!(details=%e, file=?src, "Ffprobe exec failed."); }
    };
    None
}


/// Extract exactly THUMB_COUNT frames (THUMB_W x THUMB_H, letterboxed) that cover the whole video
/// and save them as WEBP files (thumb_NN.webp) in the given directory.
/// Copy the first frame also as thumb.webp (for fast preview without seeking).
///
/// # Arguments
/// * `args` - what to process and where to put the result
///
fn run_ffmpeg_thumbnailer( args: CmprInput ) -> CmprOutput
{
    let _span = tracing::info_span!("run_ffmpeg_thumbnailer",
        video = %args.video_hash,
        user = %args.user_id,
        thread = ?std::thread::current().id()).entered();

    let thumb_dir = match args.thumb_dir.clone() {
        Some(p) => p,
        None => return err2cout("BUG: thumbnailer called with no thumb_dir", "", &args)
    };

    tracing::info!(src=%args.src.display(), dst=%thumb_dir.display(), "Thumbnailing video");

    if !thumb_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&thumb_dir) {
            return err2cout("Failed to create thumbnail directory", &e.to_string(), &args);
        }
    }

    // Create "poster" thumbnail (probably first frame, but ffmpeg can choose any)
    let single_thumb_thread = {
        let src = args.src.clone();
        let thumb_dir = thumb_dir.clone();
        std::thread::spawn(move || {
            let _span = tracing::info_span!("ffmpeg_thumb_poster_thread",
                thread = ?std::thread::current().id()).entered();

            let img_reshape = format!("scale={THUMB_W}:{THUMB_H}:force_original_aspect_ratio=decrease,pad={THUMB_W}:{THUMB_H}:(ow-iw)/2:(oh-ih)/2");

            let mut cmd = &mut Command::new("nice");
            cmd = cmd.arg("-n").arg("10").arg("--")
                .arg("ffmpeg").arg("-i").arg(&src).args(&[
                "-nostats",
                "-vcodec", "libwebp",
                "-vf", format!("thumbnail,{img_reshape}",).as_str(),
                "-frames:v", "1",
                "-strict", "experimental",
                "-c:v", "libwebp",
            ]).arg(thumb_dir.join("thumb.webp"));
            tracing::info!("Creating poster thumbnail");
            tracing::debug!("Exec: {:?}", cmd);
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

    // Create thumbnail sheet (preview of the whole video)
    let sheet_thread = {
        let src = args.src.clone();
        let thumb_dir = thumb_dir.clone();
        std::thread::spawn(move || {
            let _span = tracing::info_span!("ffmpeg_thumbsheet_thread",
                thread = ?std::thread::current().id()).entered();

                let img_reshape = format!("scale={THUMB_W}:{THUMB_H}:force_original_aspect_ratio=decrease,pad={THUMB_W}:{THUMB_H}:(ow-iw)/2:(oh-ih)/2");

                let total_frames = match count_frames(&src) {
                    Some(d) => d,
                    None => return (Some("ffprobe count_frames failed".to_string()), "".into(), "".into())
                };
        
            // Make a "-vf" filter that selects exactly THUMB_COUNT frames from the video
            let frame_select_filter = (0..THUMB_COUNT).map(|pos| {
                    let frame = (pos as f64 * (total_frames as f64 / (THUMB_COUNT as f64))) as i32;
                    format!("eq(n\\,{})", frame)
                }).collect::<Vec<String>>().join("+");


            let mut cmd = &mut Command::new("nice");
            cmd = cmd.arg("-n").arg("10").arg("--")
                .arg("ffmpeg").arg("-i").arg(&src).args(&[
                "-nostats",
                "-vf", &format!("select={frame_select_filter},{img_reshape},tile={THUMB_SHEET_COLS}x{THUMB_SHEET_ROWS}"),
                "-strict", "experimental",
                "-c:v", "libwebp",
                "-vsync", "vfr",
                "-start_number", "0",
            ]).arg(thumb_dir.join(format!("sheet-{THUMB_SHEET_COLS}x{THUMB_SHEET_ROWS}.webp")));

            tracing::info!("Creating thumbnail sheet");
            tracing::debug!("Exec: {:?}", cmd);
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
                tracing::info!("Thread '{name}' finished");
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

    CmprOutput {
        success: comb_err.is_none(),
        video_dst: None,
        thumb_dir: Some(thumb_dir),
        video_hash: args.video_hash.clone(),
        stdout: comb_stdout,
        stderr: comb_stderr,
        dmsg: DetailedMsg {
            msg: if comb_err.is_some() { "Thumbnailing failed" } else { "Thumbnailing complete" }.to_string(),
            details: format!("Error in FFMPEG: {:?}", comb_err),
            src_file: args.src.clone(),
            user_id: args.user_id.clone()
        },
        user_id: args.user_id.clone()
    }
}


/// Listen to incoming transcoding/thumbnailing requests and spawn a thread (from a pool) to handle each one.
/// Calls FFMpeg CLI to do the actual work, and sends progress updates to the given channel.
/// 
/// # Arguments
/// * `inq` - Channel to receive incoming requests
/// * `outq` - Channel to send results
/// * `progress` - Channel to send transcoding progress updates. Tuple: (video_hash, progress_msg)
/// * `n_workers` - Number of worker threads to spawn for processing. This should be at most the number of CPU cores.
pub fn run_forever(
    inq: Receiver<CmprInput>,
    outq: Sender<CmprOutput>,
    progress: ProgressSender,
    n_workers: usize)
{
    let _span = tracing::info_span!("COMPR").entered();
    tracing::info!(n_workers = n_workers, "Starting.");

    let pool = ThreadPool::new(n_workers);
    loop {
        match inq.recv() {
            Ok(args) => {
                tracing::info!("Got message: {:?}", args);
                let outq = outq.clone();
                let prgr_sender = progress.clone();
                pool.execute(move || {
                    if let Some(_) = args.video_dst {
                        if let Err(e) = outq.send(
                            run_ffmpeg_transcode(args.clone(), prgr_sender)) {
                            tracing::error!("Transcode result send failed! Aborting. -- {:?}", e);
                    }};
                    if let Some(_) = args.thumb_dir {
                        if let Err(e) = outq.send(
                            run_ffmpeg_thumbnailer(args.clone())) {
                            tracing::error!("Thumbnail result send failed! Aborting. -- {:?}", e);
                    }};
                });
            },
            Err(e) => {
                tracing::info!(details=%e, "Input queue closed.");
                break;
            }
        }
    }

    tracing::info!("Exiting.");
}
