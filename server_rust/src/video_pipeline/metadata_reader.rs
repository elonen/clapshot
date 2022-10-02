#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::process::Command;
use std::{thread, time::Duration};
use threadpool::ThreadPool;
use std::path::{Path, PathBuf};
use serde_json;
use crossbeam_channel::{Sender, Receiver, RecvError, unbounded};
use tracing;
use rust_decimal::prelude::*;

#[derive(Debug)]
pub struct Args {
    pub file_path: PathBuf,
    pub user_id: String,
}

#[derive(Debug)]
pub struct Results {
    pub success: bool,
    pub msg: String,
    pub details: String,
    pub src_file: PathBuf,
    pub user_id: String,
    pub total_frames: u32,
    pub duration: Decimal,
    pub orig_codec: String,
    pub fps: Decimal,
    pub bitrate: u32,
    pub metadata_all: String,
}

fn run_mediainfo( file: &PathBuf ) -> Result<serde_json::Value, String>
{
    match Command::new("mediainfo")
        .arg("--Output=JSON")
        .arg(file)
        .output()
    {
        Ok(output) => {
            if output.status.success() {                
                {
                    let json_res = String::from_utf8(output.stdout)
                        .map_err(|e| format!("{:?}", e))?;
                    serde_json::from_str(&json_res)
                }.map_err(|e| format!("Error parsing mediainfo JSON: {:?}", e))
            } else {
                Err( format!("Mediainfo exited with error: {}",
                    String::from_utf8(output.stderr).unwrap_or_else(|_| "Unknown error".to_string())))
            }
        },
        Err(e) => {
            Err(format!("Failed to execute mediainfo: {}", e))
        }
    }
}

fn extract_variables(args: &Args) -> Result<Results, String>
{
    let json = run_mediainfo(&args.file_path)?;
    let tracks = json["media"]["track"].as_array().ok_or("No tracks found")?;
    let video_track = tracks.iter().find(|t| t["@type"] == "Video").ok_or("No video track found")?;

    let fps = video_track["FrameRate"].as_str().ok_or("FPS not found")?;
    let duration = video_track["Duration"].as_str().ok_or("Duration not found")?;
    let frame_count = video_track["FrameCount"].as_str().ok_or("Frame count not found")?;

    Ok(Results {
        success: true,
        msg: "".to_string(),
        details: "".to_string(),
        src_file: args.file_path.clone(),
        user_id: args.user_id.clone(),
        total_frames: frame_count.parse::<u32>().map_err(|e| format!("Invalid frame count: {:?}", e))?,
        duration: Decimal::from_str(duration).map_err(|_| format!("Invalid duration: {}", fps))?,
        orig_codec: video_track["Format"].as_str().ok_or("No codec found")?.to_string(),
        fps:  Decimal::from_str(fps).map_err(|_| format!("Invalid FPS: {}", fps))?,
        bitrate: video_track["BitRate"].as_str().unwrap_or("0").parse::<u32>().unwrap_or(0),
        metadata_all: json.to_string()
    })
}

#[tracing::instrument]
fn process_one_file(args: Args, outq: &Sender<Results>) -> Result<(), String>
{
    let res = extract_variables(&args);
    match res {
        Ok(r) => {
            outq.send(r).map_err(|e| format!("Failed to send results: {:?}", e))
        },
        Err(e) => {
            outq.send(Results {
                success: false,
                msg: "Metadata read failed".to_string(),
                details: e,
                src_file: args.file_path.clone(),
                user_id: args.user_id.clone(),
                total_frames: 0,
                duration: Decimal::zero(),
                orig_codec: "".to_string(),
                fps: Decimal::zero(),
                bitrate: 0,
                metadata_all: "".to_string()
            }).map_err(|e| format!("Failed to send results: {:?}", e))
        }
    }
}


pub fn run_forever(inq: Receiver<Args>, outq: Sender<Results>, n_workers: usize)
{
    tracing::info!("Starting.");
    let pool = ThreadPool::new(n_workers);

    loop {
        match inq.recv() {
            Ok(args) => {
                tracing::info!("Got message: {:?}", args);

                let outq_copy = outq.clone();
                pool.execute(move || {
                    match process_one_file(args, &outq_copy) {
                        Ok(_) => {},
                        Err(e) => {
                            tracing::error!("Metadata worker error: {:?}", e);
                        }
                    }
                });                
            },
            Err(RecvError) => {
                tracing::info!("Channel closed. Exiting.");
                break;
            }
        }
    }

    tracing::warn!("Clean exit.");
}
