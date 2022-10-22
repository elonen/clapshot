use std::{process::Command, io::BufRead};
use std::path::{PathBuf};
use crossbeam_channel::{Sender, Receiver};
use tracing;
use threadpool::ThreadPool;

use super::DetailedMsg;

pub type ProgressSender = crossbeam_channel::Sender<(String, String)>;


#[derive(Debug, Clone)]
pub struct CmprInput {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub video_bitrate: u32,
    pub video_hash: String,
    pub user_id: String,
}

#[derive(Debug, Clone)]
pub struct CmprOutput {
    pub success: bool,
    pub dst_file: String,
    pub video_hash: String,
    pub stdout: String,
    pub stderr: String,
    pub dmsg: DetailedMsg,
}

fn err2cout<E: std::fmt::Debug>(msg_txt: &str, err: E, args: &CmprInput) -> CmprOutput {
    CmprOutput {
        success: false,
        dst_file: args.dst.to_str().unwrap_or("<invalid path>").to_string(),
        video_hash: args.video_hash.clone(),
        stdout: "".into(),
        stderr: "".into(),
        dmsg: DetailedMsg {
            msg: msg_txt.to_string(),
            details: format!("{:?}", err),
            src_file: args.src.clone(),
            user_id: args.user_id.clone()
        },
    }
}

/// Use ffprobe to find how many frames are in the video
/// 
/// # Arguments
/// * `file_path` - Path to the file to be analyzed
/// # Returns
/// * Number of frames in the video, or -1 on error
fn count_frames( src: &str ) -> i32
{
    // Equiv to: ffprobe -v error -select_streams v:0 -count_packets -show_entries stream=nb_read_packets -of csv=p=0 <INPUT-FILE>
    let cmd_res = Command::new("ffprobe")
        .args(&["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0"])
        .arg(&src).output();
    match cmd_res {
        Ok(output) => {
            if output.status.success() {
                match String::from_utf8_lossy(&output.stdout).trim().parse::<u32>() {
                    Ok(n) => { return n as i32; },
                    Err(e) => { tracing::error!("Frame counting '{}' failed: {}", src, e); }
                }
            } else { tracing::error!("Frame counting '{}' failed; ffprobe exited with error: {}", src, String::from_utf8_lossy(&output.stderr)); }
        },
        Err(e) => { tracing::error!("Frame counting '{}' failed: {}", src, e); }
    };
    -1
}

/// Run FFMpeg shell command and return the output (stdout, stderr)
/// Send progress updates to the progress channel.
/// 
/// # Arguments
/// * `args` - what to compress and where to put the result
/// * `progress` - channel to send progress updates to
/// 
fn run_ffmpeg( args: CmprInput, progress: ProgressSender ) -> CmprOutput
{
    let src_str = match args.src.to_str() {
        Some(s) => s,
        None => return err2cout("Invalid src path", "", &args) }.to_string();

    let dst_str = match args.dst.to_str() {
        Some(s) => s,
        None => return err2cout("Invalid dst path", "", &args) }.to_string();

    // Open a named pipe for ffmpeg to write progress reports to.
    // If this fails, ignore it and just don't show progress.
    let (ppipe_fname, ppipe_f) = {
        let fname = args.dst.with_extension("progress").with_extension("pipe");
        match fname.to_str() {
            None => { Err("Invalid dst path".to_string()) }
            Some(fname) => std::fs::File::create(&fname)
                .map(|f| (fname.to_string(), f))
                .map_err(|e| e.to_string())
        }.map_or_else(
            |e| { tracing::warn!("Won't track FFMPEG progress; failed to create pipe file: {}", e); (None, None) },
             |f| (Some(f.0), Some(f.1)))
    };
    
    // Start encoder thread
    let ffmpeg_thread = {
        let src_str = src_str.clone();
        let dst_str = dst_str.clone();
        std::thread::spawn(move || {
            // ffmpeg -i INPUT.MOV  -progress - -nostats -vcodec libx265 -vf scale=1280:-8 -map 0 -acodec aac -ac 2 -strict experimental -b:v 2500000 -b:a 128000 OUTPUT.mp4
            let mut cmd = &mut Command::new("ffmpeg");
            cmd = cmd.args(&["-i", &src_str]);
            if let Some(pipe,) = ppipe_fname {
                cmd = cmd.args(&["-progress", &pipe]);
            }
            cmd = cmd.args(&[
                "-nostats",
                "-vcodec", "libx265",
                "-vf", &format!("scale={}:{}", 1920, -8),
                "-map", "0",
                "-acodec", "aac",
                "-ac", "2",
                "-strict", "experimental",
                "-b:v", &format!("{}", args.video_bitrate),
                "-b:a", &format!("{}", 128000),
                &dst_str
            ]);
            match cmd.output() {
                Ok(res) => {(
                    if res.status.success() {None} else {Some("FFMPEG exited with error".to_string())},
                    String::from_utf8_lossy(&res.stdout).to_string(),
                    String::from_utf8_lossy(&res.stderr).to_string(),
                )},
                Err(e) => {
                    (Some(e.to_string()), "".into(), "".into())
                }
            }
        })};

    let progress_terminate = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Thread to read FFMPEG progress report from named pipe and send updates to user(s)
    let progress_thread =
    {
        let progress_terminate = progress_terminate.clone();
        match ppipe_f {
            None => std::thread::spawn(move || {}),
            Some(pipe) => {
                let vh = args.video_hash.clone();
                let src_str = src_str.clone();
                std::thread::spawn(move || {
                    let total_frames = count_frames(&src_str);
                    let reader = &mut std::io::BufReader::new(&pipe);

                    let mut msg : Option<String> = None;
                    let mut frame = -1;
                    let mut fps = -1f32;

                    while !progress_terminate.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        // Read progress lines from pipe & parse
                        match reader.lines().next() {
                            Some(Err(e)) => { tracing::debug!("Error reading progress pipe: {}", e); break; },
                            None => { tracing::debug!("Progress pipe EOF"); break; }
                            Some(Ok(l)) => {
                                // FFMPEG Progress "chunk" looks like this:
                                //    frame=584
                                //    fps=52.40
                                //       <...some other key-value pairs...>
                                //    progress=continue
                                match l.find("=") {
                                    None => { tracing::debug!("Skipping invalid FFMPEG progress log line: '{}'", l); }
                                    Some(idx) => {
                                        let (key, val) = l.split_at(idx);
                                        let val = &val[1..];
                                        match key {
                                            "frame" => {
                                                match val.parse::<i32>() {
                                                    Ok(n) => { frame = n; },
                                                    Err(e) => { tracing::warn!("Invalid frame# in FFMPEG progress log: {}", e); }
                                                }},
                                            "fps" => {
                                                match val.parse::<f32>() {
                                                    Ok(n) => { fps = n; },
                                                    Err(e) => { tracing::warn!("Invalid fps in FFMPEG progress log: {}", e); }
                                                }},
                                            "progress" => {
                                                match val {
                                                    "end" => { msg = Some("Transcoding done.".to_string()); },
                                                    _ => {
                                                        let fps_str = if fps > 0f32 { format!(" (speed: {:.1} fps)", fps) } else { "".to_string() };
                                                        if frame >= 0 && total_frames > 0 {
                                                                let ratio = frame as f32 / total_frames as f32;
                                                                msg = Some(format!("Transcoding... {:.1}% done{fps_str}", (ratio * 100f32) as i32));
                                                        } else {
                                                            msg = Some("Transcoding...{fps_str}".to_string()); 
                                                        }}}},
                                            _ => {}, // Ignore other keys
                                        }}}
                                // Send progress message (if any)
                                if let Some(msg) = msg.take() {
                                    if let Err(e) = progress.send((vh.clone(), msg)) {
                                        tracing::debug!("Failed to send FFMPEG progress message. Ending progress tracking. -- {:?}", e);
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
            tracing::error!("FFMPEG thread panicked: {:?}", e);
            (Some("FFMPEG thread panicked".to_string()), "".into(), format!("{:?}", e))
        }
    };
    progress_terminate.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Err(e) = progress_thread.join() {
        tracing::warn!("FFMPEG progress reporter thread panicked (ignoring): {:?}", e);
    }

    CmprOutput {
        success: err_msg.is_none(),
        dst_file: dst_str,
        video_hash: args.video_hash.clone(),
        stdout: stdout,
        stderr: stderr,
        dmsg: DetailedMsg {
            msg: if err_msg.is_some() { "Transcoding failed" } else { "Transcoding complete" }.to_string(),
            details: format!("Error in FFMPEG: {:?}", err_msg),
            src_file: args.src.clone(),
            user_id: args.user_id.clone()
        },
    }
}

/// Listen to incoming transcoding requests and spawn a thread (from a pool) to handle each one.
/// Calls FFMpeg CLI to do the actual transcoding, and sends progress updates to the given channel.
/// 
/// # Arguments
/// * `inq` - Channel to receive incoming transcoding requests
/// * `outq` - Channel to send transcoding results
/// * `progress` - Channel to send transcoding progress updates. Tuple: (video_hash, progress_msg)
/// * `n_workers` - Number of worker threads to spawn for encoding. This should be at most the number of CPU cores.
pub fn run_forever(
    inq: Receiver<CmprInput>,
    outq: Sender<CmprOutput>,
    progress: ProgressSender,
    n_workers: usize)
{
    tracing::info!("Starting.");
    let pool = ThreadPool::new(n_workers);

    loop {
        match inq.recv() {
            Ok(args) => {
                tracing::info!("Got message: {:?}", args);
                let outq = outq.clone();
                let prgr_sender = progress.clone();
                pool.execute(move || {
                    if let Err(e) = outq.send(
                        run_ffmpeg(args.clone(), prgr_sender))
                    {
                        tracing::error!("Result send failed! Aborting. -- {:?}", e);
                    }});
            },
            Err(e) => {
                tracing::info!("Input queue closed ({e}). Exiting.");
                break;
            }
        }
    }

    tracing::info!("Clean exit.");
}
