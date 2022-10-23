#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]


use std::error;
use std::{path::PathBuf, str::FromStr};
use std::{thread, time::Duration};

use assert_fs::prelude::PathCopy;
use rust_decimal::prelude::*;

use crossbeam_channel;
use crossbeam_channel::{Receiver, RecvTimeoutError, unbounded, select};

use clapshot_server::video_pipeline::{metadata_reader, IncomingFile};
use tracing;

use tracing::{error, info, warn, instrument};
use tracing_test::traced_test;

type Result<T> = std::result::Result<T, Box<dyn error::Error>>;



#[test]
#[traced_test]
fn test_integ_metadata_reader_ok() -> Result<()>
{
    let data_dir = assert_fs::TempDir::new()?;
    data_dir.copy_from("tests/assets/", &["*.mov"])?;

    // Show copied files
    for entry in data_dir.path().read_dir()? {
        tracing::info!("COPIED FILE: {:?}", entry?.path());
    }

    let (arg_sender, arg_recvr) = unbounded::<IncomingFile>();
    let (res_sender, res_recvr) = unbounded::<metadata_reader::MetadataResult>();
    let th = thread::spawn(move || {
            metadata_reader::run_forever(arg_recvr, res_sender, 4);
        });

    // Send request to metadata reader
    let args = IncomingFile {
        file_path: PathBuf::from_str(data_dir.join("NASA_Red_Lettuce_excerpt.mov").to_str().unwrap())?,
        user_id: "nobody".to_string(),
    };
    arg_sender.send(args.clone())?;

    // Wait for response
    let res = res_recvr.recv_timeout(Duration::from_secs(5))?.unwrap();
    tracing::info!("Got response: {:?}", res);

    drop(arg_sender);
    drop(res_recvr);
    th.join().unwrap();

    assert_eq!(res.user_id, "nobody");
    assert_eq!(res.src_file, args.file_path);
    assert_eq!(res.total_frames, 123);
    assert_eq!(res.fps, Decimal::from_str("23.976")?);
    //assert!(logs_contain("Clean exit"));

    data_dir.close().unwrap();
    Ok(())
}
