#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::str::FromStr;
use std::{thread, time::Duration};
use std::path::PathBuf;

use crossbeam_channel;
use crossbeam_channel::{Receiver, unbounded, select};

use tracing;

pub mod incoming_monitor;
pub mod metadata_reader;

use metadata_reader::MetadataResult;

use crate::api_server::UserMessage;


#[derive(Debug)]
pub struct Args {
    msg: String
}

#[tracing::instrument]
fn do_stuff(i: &mut i32) {
    thread::sleep(Duration::from_secs(1));
    tracing::info!("Worker says {}", i);
    *i += 1;
}

pub fn run_forever(
    data_dir: PathBuf,
    user_msg_tx: crossbeam_channel::Sender<UserMessage>,
    poll_interval: f32, resubmit_delay: f32,
    inq: Receiver<Args>)
{
    tracing::info!("Starting video processing pipeline. Polling interval: {}s, resubmit delay: {}s", poll_interval, resubmit_delay);

    // Thread for incoming monitor
    let (_md_thread, from_md, to_md) = {
            let (arg_sender, arg_recvr) = unbounded::<metadata_reader::Args>();
            let (res_sender, res_recvr) = unbounded::<MetadataResult>();

            let th = thread::spawn(move || {
                    metadata_reader::run_forever(arg_recvr, res_sender, 4);
                });
            (th, res_recvr, arg_sender)
        };
    // TEST: post a file read
    to_md.send(metadata_reader::Args{
        file_path: PathBuf::from_str("../../guest_playthrough_220922.mkv").unwrap(),
        user_id: "nobody".to_string()}).unwrap();
    

    // Thread for metadata reader
    let (mon_thread, from_mon, mon_exit) = {
        let (sender, recvr) = unbounded::<String>();
        let (exit_sender, exit_recvr) = unbounded::<incoming_monitor::Void>();

        let th = thread::spawn(move || {
                incoming_monitor::run_forever(
                    (data_dir.join("src") ).clone(),
                    poll_interval, resubmit_delay,
                    sender, exit_recvr);
            }); 
        (th, recvr, exit_sender)
    };


    loop {
        select! {
            // Ehhh... something??
            recv(inq) -> msg => {
                match msg {
                    Ok(msg) => tracing::info!("A message was received from inq: {:?}", msg),
                    Err(_) => { break; }
                }
            }
            // Metadata reader
            recv(from_md) -> msg => {
                match msg {
                    Ok(md_res) => { 
                        tracing::info!("Got metadata result: {:?}", md_res);
                        match md_res {
                            MetadataResult::Ok(md) => {
                                // TODO: pass along to video ingestion
                            }
                            MetadataResult::Err(e) => {
                                let details = format!("File: '{:?}'. Error: {} -- {}", e.src_file.file_name(), e.msg, e.details);
                                tracing::error!("Metadata reader error: {}", details);
                                user_msg_tx.send(UserMessage {
                                        msg: format!("Error reading video metadata."),
                                        details: Some(details),
                                        user_id: e.user_id,
                                    }).ok();
                            }
                        }
                    },
                    Err(e) => { tracing::warn!("Metadata reader is dead ('{:?}'). Aborting.", e); break; },
                }
            },
            // Incoming monitor
            recv(from_mon) -> msg => {
                match msg {
                    Ok(new_file) => tracing::info!("New file found: {:?}", new_file),
                    Err(e) => { tracing::warn!("Metadata reader is dead ('{:?}'). Aborting.", e); break; },
                }
            },
        }
        if mon_thread.is_finished() {
            tracing::error!("Incoming monitor thread is dead. Aborting.");
            break;
        }
    }


    drop(mon_exit);
    match mon_thread.join() {
        Ok(_) => {},
        Err(e) => {
            tracing::error!("Error waiting for monitor thread to exit: {:?}", e);
        }
    }

    tracing::warn!("Clean exit.");
}
