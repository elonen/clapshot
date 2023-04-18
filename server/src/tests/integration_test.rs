#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

#[cfg(test)]
mod integration_test
{
    use std::{error, any};
    use std::{path::PathBuf, str::FromStr};
    use std::{thread, time::Duration};

    use assert_fs::prelude::PathCopy;
    use futures::Future;
    use rust_decimal::prelude::*;

    use crossbeam_channel;
    use crossbeam_channel::{Receiver, RecvTimeoutError, unbounded, select};

    use crate::api_server::tests::expect_user_msg;
    use crate::database::schema::videos::{thumb_sheet_cols, thumb_sheet_rows};
    use crate::expect_client_cmd;
    use crate::video_pipeline::{metadata_reader, IncomingFile};
    use crate::api_server::test_utils::{connect_client_ws, open_video, write};
    use lib_clapshot_grpc::{GrpcBindAddr, proto};
    use lib_clapshot_grpc::proto::client::ServerToClientCmd;
    use lib_clapshot_grpc::proto::client::server_to_client_cmd as s2c;

    use tracing;
    use tracing::{error, info, warn, instrument};
    use tracing_test::traced_test;


    #[test]
    #[traced_test]
    fn test_integ_metadata_reader_ok() -> anyhow::Result<()>
    {
        let data_dir = assert_fs::TempDir::new()?;
        data_dir.copy_from("src/tests/assets/", &["*.mov"])?;

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



    macro_rules! cs_main_test {
        ([$ws:ident, $data_dir:ident, $incoming_dir:ident, $bitrate:expr] $($body:tt)*) => {
            {
                let $data_dir = assert_fs::TempDir::new().unwrap();
                let $incoming_dir = $data_dir.join("incoming");

                // Run server
                let port = portpicker::pick_unused_port().expect("No TCP ports free");
                let url_base = format!("http://127.0.0.1:{}", port);
                let ws_url = format!("{}/api/ws", &url_base.replace("http", "ws"));
                let target_bitrate = $bitrate;
                let grpc_server_bind = GrpcBindAddr::Unix($data_dir.path().join("grpc-org-to-srv-TEST.sock").into());
                let th = {
                    let poll_interval = 0.1;
                    let data_dir = $data_dir.path().to_path_buf();
                    let url_base = url_base.clone();
                    thread::spawn(move || {
                        crate::run_clapshot(data_dir, true, url_base, "127.0.0.1".into(), port, None, grpc_server_bind, 4, target_bitrate, poll_interval, poll_interval*5.0).unwrap()
                    })};
                thread::sleep(Duration::from_secs_f32(0.25));

                let resp = reqwest::blocking::get(&format!("{}/api/health", &url_base)).unwrap();
                assert_eq!(resp.status(), 200);

                tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                    // Connect client
                    let cur_process_user = whoami::username();
                    let mut $ws = connect_client_ws(&ws_url, &cur_process_user).await;

                    { $($body)* }
                });
            }
        }
    }

    #[test]
    #[traced_test]
    fn test_video_ingest_no_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, 2500_000]
            // Copy test file to incoming dir
            let mp4_file = "60fps-example.mp4";
            data_dir.copy_from("src/tests/assets/", &[mp4_file]).unwrap();
            std::fs::rename(data_dir.join(mp4_file), incoming_dir.join(mp4_file)).unwrap();

            // Wait for file to be processed
            thread::sleep(Duration::from_secs_f32(0.5));
            let m = expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;
            let vid = m.refs.unwrap().video_id.unwrap();

            crate::api_server::test_utils::wait_for_thumbnails(&mut ws).await;

            // Open video from server and check metadata
            let cideo = open_video(&mut ws, &vid).await.video.unwrap();
            assert_eq!(cideo.processing_metadata.unwrap().orig_filename.as_str(), mp4_file);

            // Double slashes in the path are an error (empty path component)
            let video_url = cideo.playback_url.unwrap();
            let after_https = video_url.split("://").nth(1).unwrap();
            assert!(!after_https.contains("//"));

            // Check that video was moved to videos dir and symlinked
            assert!(data_dir.path().join("videos").join(&vid).join("orig").join(mp4_file).is_file());
            assert!(!incoming_dir.join(mp4_file).exists());

            // Add a comment
            let msg = serde_json::json!({"cmd": "add_comment", "data": { "video_id": vid, "comment": "Test comment"}});
            write(&mut ws, &msg.to_string()).await;
            let mut got_new_comment = false;
            for _ in 0..3 {
                match crate::api_server::test_utils::try_get_parsed::<ServerToClientCmd>(&mut ws).await.map(|x| x.cmd.unwrap()) {
                    Some(proto::client::server_to_client_cmd::Cmd::AddComments(m)) => {
                        got_new_comment = true;
                        break;
                    },
                    _ => {},
                }
            }
            assert!(got_new_comment);
        }
        Ok(())
    }

    #[test]
    #[traced_test]
    fn test_video_ingest_corrupted_video() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, 500_000]
            tracing::info!("WRITING CORRUPTED VIDEO");

            // Copy test file to incoming dir
            let f = incoming_dir.join("garbage.mp4");
            std::fs::File::create(&f).unwrap().set_len(123000).unwrap();

            // Wait for file to be processed
            thread::sleep(Duration::from_secs_f32(0.5));

            // Expect error
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
            assert!(msg.details.unwrap().contains("garbage.mp4"));

            // Make sure video was moved to rejected dir
            assert!(!f.exists());
            assert!(data_dir.path().join("rejected").join("garbage.mp4").exists());
        }
        Ok(())
    }

    #[test]
    #[traced_test]
    fn test_video_ingest_and_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, 500_000]
            // Copy test file to incoming dir
            let mov_file = "NASA_Red_Lettuce_excerpt.mov";
            let dangerous_name = "  -fake-arg name; \"and some more'.txt 你 .mov";
            data_dir.copy_from("src/tests/assets/", &[mov_file]).unwrap();
            std::fs::rename(data_dir.join(mov_file), incoming_dir.join(dangerous_name)).unwrap();

            // Wait for file to be processed
            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;
            let vid = msg.refs.unwrap().video_id.unwrap();

            // Check that it's being transcoded
            assert!(msg.details.unwrap().to_ascii_lowercase().contains("ranscod"));
            assert!(vid.len() > 0);

            // Wait until transcoding is done
            let mut transcode_complete = false;
            let mut got_progress_report = false;
            let mut ts_cols = String::new();
            let mut ts_rows = String::new();

            let mut got_video_updated = false;

            'waitloop: for _ in 0..(120*5)
            {
                if !got_video_updated {
                    got_video_updated |= match crate::api_server::test_utils::try_get_parsed::<ServerToClientCmd>(&mut ws).await
                        .map(|c| c.cmd).flatten() {
                            Some(s2c::Cmd::ShowMessages(m)) => {
                                got_progress_report |= m.msgs.iter().any(|msg| msg.r#type == proto::user_message::Type::Progress as i32);
                                // return true if we got a VideoUpdated message (which is sent when video is done transcoding)
                                m.msgs.iter().any(|msg| msg.r#type == proto::user_message::Type::VideoUpdated as i32)
                            },
                            _ => { false }
                        };
                    if !got_video_updated {
                        println!("...still waiting for VideoUpdated...");
                        thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                }

                println!("... doing list_my_videos ...");
                write(&mut ws, r#"{"cmd":"list_my_videos","data":{}}"#).await;

                match crate::api_server::test_utils::expect_parsed::<ServerToClientCmd>(&mut ws).await.cmd {

                    Some(s2c::Cmd::ShowMessages(m)) => {
                        got_progress_report |= m.msgs.iter().any(|msg| msg.r#type == proto::user_message::Type::Progress as i32);
                    },

                    Some(s2c::Cmd::ShowPage(p)) => {
                        let pitems = p.page_items;
                        assert!(pitems.len() == 1+1);

                        match &pitems[0].item {
                            Some(proto::page_item::Item::Html(_)) => {},
                            _ => panic!("Expected HTML for page item 0"),
                        };

                        let fl = match &pitems[1].item {
                            Some(proto::page_item::Item::FolderListing(fl)) => fl,
                            _ => panic!("Expected folder listing for page item 1"),
                        };
                        let v = match fl.items[0].item.clone().unwrap() {
                            proto::page_item::folder_listing::item::Item::Video(v) => v,
                            _ => panic!("Expected video"),
                        };
                        assert_eq!(v.id, vid);

                        if let Some(pd) = v.preview_data {
                            if let Some(pm) = v.processing_metadata {
                                if pm.recompression_done.is_some() {
                                    transcode_complete = true;
                                    let thumb_sheet = pd.thumb_sheet.unwrap();
                                    ts_cols = thumb_sheet.cols.to_string();
                                    ts_rows = thumb_sheet.rows.to_string();
                                    break 'waitloop;
                                }
                            }
                        }
                    },
                    _ => {},
                }

                thread::sleep(Duration::from_secs_f32(0.2));
            }

            assert!(transcode_complete, "Transcode did not complete / was not marked done");
            assert!(got_progress_report);

            let vid_dir = data_dir.path().join("videos").join(vid);
            assert!(vid_dir.join("video.mp4").is_symlink());
            assert!(vid_dir.join("stdout.txt").is_file());
            assert!(vid_dir.join("stderr.txt").is_file());
            assert!(vid_dir.join("orig").join(dangerous_name).is_file());

            let thumb_dir = vid_dir.join("thumbs");
            assert!(thumb_dir.join("thumb.webp").is_file());
            assert!(thumb_dir.join(format!("sheet-{ts_cols}x{ts_rows}.webp")).is_file());
            assert!(thumb_dir.join("stdout.txt").is_file());
            assert!(thumb_dir.join("stderr.txt").is_file());
        }
        Ok(())
    }

}
