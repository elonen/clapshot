#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

#[cfg(test)]
mod integration_test
{
    use std::collections::HashMap;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Mutex, Arc};
    use std::{error, any};
    use std::{path::PathBuf, str::FromStr};
    use std::{thread, time::Duration};

    use assert_fs::prelude::PathCopy;
    use chrono::format;
    use futures::Future;
    use lib_clapshot_grpc::proto::client::client_to_server_cmd::{AddComment, OpenNavigationPage};
    use lib_clapshot_grpc::proto::org::{self, RunTestResponse};
    use rust_decimal::prelude::*;

    use crossbeam_channel;
    use crossbeam_channel::{Receiver, RecvTimeoutError, unbounded, select};

    use crate::api_server::tests::expect_user_msg;
    use crate::database::schema::media_files::{thumb_sheet_cols, thumb_sheet_rows};
    use crate::{expect_client_cmd, send_server_cmd};
    use crate::grpc::grpc_client::prepare_organizer;
    use crate::video_pipeline::{metadata_reader, IncomingFile};
    use crate::api_server::test_utils::{connect_client_ws, open_media_file, write};
    use lib_clapshot_grpc::{GrpcBindAddr, proto};
    use lib_clapshot_grpc::proto::client::ServerToClientCmd;
    use lib_clapshot_grpc::proto::client::server_to_client_cmd as s2c;

    use tracing;
    use tracing::{error, info, warn, instrument};
    use tracing_test::traced_test;
    use serial_test::serial;
    use std::io::Write;


    #[test]
    #[serial]
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
            cookies: HashMap::new(),
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


    /// Query API health endpoint until it returns 200 OK or timeout
    fn wait_for_healthy(url_base: &str) -> bool {
        const MAX_RETRIES: usize = 10;
        let mut interval_ms: u64 = 10;
        let url = format!("{}/api/health", url_base);
        for i in 1..=MAX_RETRIES {
            if i > 1 { thread::sleep(Duration::from_millis(interval_ms)); }
            interval_ms = std::cmp::min(interval_ms * 2, 1000);
            let resp_result = reqwest::blocking::get(&url);
            if let Ok(resp) = resp_result {
                if resp.status() == 200 { return true; }
                else { tracing::debug!("wait_for_healthy got status {} from /api/health. Try {}/{}.", resp.status(), i, MAX_RETRIES) }
            }
        }
        false
    }

    macro_rules! cs_main_test {
        ([$ws:ident, $data_dir:ident, $incoming_dir:ident, $org_conn:ident, $bitrate:expr, $org_cmd:expr, $custom_assertfs:expr] $($body:tt)*) => {
            {
                let $data_dir = $custom_assertfs.unwrap_or(assert_fs::TempDir::new().unwrap());
                let $incoming_dir = $data_dir.join("incoming");
                std::fs::create_dir($incoming_dir.as_path()).ok();

                // Run server
                let port = portpicker::pick_unused_port().expect("No TCP ports free");
                let url_base = format!("http://127.0.0.1:{}", port);
                let ws_url = format!("{}/api/ws", &url_base.replace("http", "ws"));
                let target_bitrate = $bitrate;

                let grpc_server_bind = crate::grpc::grpc_server::make_grpc_server_bind(&None, &$data_dir)?;
                let (org_uri, _org_hdl) = prepare_organizer(&None, &$org_cmd, tracing::Level::DEBUG, false, &$data_dir.path())?;

                let terminate_flag = Arc::new(AtomicBool::new(false));

                let th = {
                    let poll_interval = 0.1;
                    let data_dir = $data_dir.path().to_path_buf();
                    let url_base = url_base.clone();
                    let org_uri = org_uri.clone();
                    let tf = terminate_flag.clone();
                    thread::spawn(move || {
                        let mut clapshot = crate::ClapshotInit::init_and_spawn_workers(data_dir, true, url_base, vec![], "127.0.0.1".into(), port, org_uri.clone(), grpc_server_bind, 4, target_bitrate, poll_interval, "anonymous".to_string(), poll_interval*5.0, tf)?;
                        clapshot.wait_for_termination()
                })};

                assert!(wait_for_healthy(&url_base), "Server API never became healthy");

                tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
                    // Connect client
                    let cur_process_user = whoami::username();
                    let mut $ws = connect_client_ws(&ws_url, &cur_process_user).await;
                    let $org_conn = match org_uri.clone() {
                        Some(org_uri) => Some(crate::grpc::grpc_client::connect(org_uri.clone()).await.expect("Failed to connect to organizer")),
                        None => None,
                    };
                    { $($body)* }
                });

                terminate_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                tracing::info!("Waiting for run_clapshot() to terminate...");
                let _ = th.join().unwrap();
            }
        }
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_video_ingest_no_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 2500_000, None, None]
            // Copy test file to incoming dir
            let mp4_file = "60fps-example.mp4";
            data_dir.copy_from("src/tests/assets/", &[mp4_file]).unwrap();
            std::fs::rename(data_dir.join(mp4_file), incoming_dir.join(mp4_file)).unwrap();

            // Wait for file to be processed
            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::MediaFileAdded).await;    // notification to client (with upload folder info etc)
            let vid = msg.refs.unwrap().media_file_id.unwrap();

            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;    // notification to user (in text)
            let vid2 = msg.refs.unwrap().media_file_id.unwrap();
            assert_eq!(vid, vid2);

            crate::api_server::test_utils::wait_for_thumbnails(&mut ws).await;

            // Open media file from server and check metadata
            let media_file = open_media_file(&mut ws, &vid).await.media_file.unwrap();
            assert_eq!(media_file.processing_metadata.unwrap().orig_filename.as_str(), mp4_file);

            // Double slashes in the path are an error (empty path component)
            let media_url = media_file.playback_url.unwrap();
            let after_https = media_url.split("://").nth(1).unwrap();
            assert!(!after_https.contains("//"));

            let orig_url = media_file.orig_url.unwrap();
            assert!(orig_url == media_url);  // No transcoding, so should be the same

            // Check that media file was moved to the media dir and symlinked
            assert!(data_dir.path().join("videos").join(&vid).join("orig").join(mp4_file).is_file());
            assert!(!incoming_dir.join(mp4_file).exists());

            // Add a comment
            send_server_cmd!(ws, AddComment, AddComment { media_file_id: vid, comment: "Test comment".to_string(), ..Default::default() });

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
    #[serial]
    #[traced_test]
    fn test_video_try_ingest_corrupted_video() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 500_000, None, None]
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


    // --- Transcoding tests ---

    pub struct WaitForReportResults {
        pub transcode_complete: bool,
        pub thumbs_complete: bool,
        pub got_progress_report: bool,
        pub got_transcode_report: bool,
        pub got_thumbnail_report: bool,
        pub ts_cols: String,
        pub ts_rows: String,
    }

    async fn wait_for_reports(
        mut ws: &mut crate::api_server::test_utils::WsClient,
        expect_transcode: bool,
        expect_thumbnail: bool,
        expect_thumbsheet: bool,
        check_file_outputs: Option<(PathBuf, String)>) -> WaitForReportResults
    {
        let mut res = WaitForReportResults {
            transcode_complete: false, thumbs_complete: false,
            got_progress_report: false, got_transcode_report: false, got_thumbnail_report: false,
            ts_cols: String::new(), ts_rows: String::new(),
        };

        const WAIT_AFTER_REPORTS_TIMEOUT_SECS: u32 = 5;

        // Wait for file to be processed
        thread::sleep(Duration::from_secs_f32(0.5));
        let msg = expect_user_msg(&mut ws, proto::user_message::Type::MediaFileAdded).await;    // notification to client (with upload folder info etc)
        let vid = msg.refs.unwrap().media_file_id.unwrap();

        thread::sleep(Duration::from_secs_f32(0.5));
        let msg = expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;    // notification to user (in text)
        let vid2 = msg.refs.unwrap().media_file_id.unwrap();
        assert_eq!(vid, vid2);

        assert!(vid.len() > 0);
        if expect_transcode {
            assert!(msg.details.unwrap().to_ascii_lowercase().contains("transcod"));
        }

        for _ in 0..(60*2*10)
        {
            // Wait until server sends media updated messages about
            // transcoding and thumbnail generation being done
            // before we try to open and check metadata.
            let mut still_waiting = true;
            if still_waiting {
                match crate::api_server::test_utils::try_get_parsed::<ServerToClientCmd>(&mut ws).await.map(|c| c.cmd).flatten() {
                    Some(s2c::Cmd::ShowMessages(m)) => {
                        // Got progress report?
                        res.got_progress_report |= m.msgs.iter().any(|msg| msg.r#type == proto::user_message::Type::Progress as i32);

                        assert!(!m.msgs.iter().any(|msg| msg.r#type == proto::user_message::Type::Error as i32), "Got ERROR type message while waiting for transcode/thumbnail completion");

                        if m.msgs.iter().any(|msg| msg.r#type == proto::user_message::Type::MediaFileUpdated as i32) {
                            // Got transcoding update message?
                            if m.msgs.iter().any(|msg| msg.clone().message.to_ascii_lowercase().contains("transcod")) {
                                res.got_transcode_report = true;
                            }
                            // Got thumbnail update message?
                            else if m.msgs.iter().any(|msg| msg.clone().message.to_ascii_lowercase().contains("thumb")) {
                                res.got_thumbnail_report = true;
                            }
                        }
                    },
                    _ => (),
                };

                still_waiting = false;
                if (expect_thumbnail || expect_thumbsheet) && !res.got_thumbnail_report {
                    println!("...still waiting for thumbnail...");
                    still_waiting = true;
                }
                if expect_transcode && !res.got_transcode_report {
                    println!("...still waiting for transcode...");
                    still_waiting = true;
                }

                if still_waiting {
                    thread::sleep(Duration::from_millis(100));
                } else {
                    println!("...waiting done, expected reports received. Doing OpenNavigationPage ...");
                    send_server_cmd!(ws, OpenNavigationPage, OpenNavigationPage {..Default::default()});
                    break;
                }
            }
        }

        let reports_received_at = std::time::Instant::now();

        // Wait for page with media file to be shown
        'waitloop: for _ in 0..80
        {
            if reports_received_at.elapsed().as_millis() > (WAIT_AFTER_REPORTS_TIMEOUT_SECS*1000).into() {
                panic!("Timeout checking API messages after transcode/thumbnail completion");
            }

            match crate::api_server::test_utils::expect_parsed::<ServerToClientCmd>(&mut ws).await.cmd {

                Some(s2c::Cmd::ShowMessages(m)) => {
                    tracing::info!("Got ShowMessages (while waiting for ShowPage. Ignoring.");
                    res.got_progress_report |= m.msgs.iter().any(|msg| msg.r#type == proto::user_message::Type::Progress as i32);
                    assert!(!m.msgs.iter().any(|msg| msg.r#type == proto::user_message::Type::Error as i32), "Got ERROR type message while waiting for ShowPage");
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
                        proto::page_item::folder_listing::item::Item::MediaFile(v) => v,
                        _ => panic!("Expected media file"),
                    };
                    assert_eq!(v.id, vid);

                    let playback_url = v.playback_url.unwrap();
                    let orig_url = v.orig_url.unwrap();
                    assert!(orig_url != playback_url);
                    assert!(orig_url.contains("orig"));
                    if expect_transcode {
                        assert!(playback_url.contains("video.mp4"));
                        assert!(!playback_url.contains("orig"));
                    } else {
                        assert!(playback_url.contains("orig"));
                        assert!(!playback_url.contains("video.mp4"));
                    }

                    if let Some(pm) = v.processing_metadata {
                        if pm.recompression_done.is_some() {
                            res.transcode_complete = true;
                        }

                        if let Some(pd) = v.preview_data {
                            if let Some(thumb_url) = pd.thumb_url {
                                assert!(pm.thumbs_done.is_some(), "thumbs_done not set in processing metadata but got thumb_url");
                                res.thumbs_complete = true;
                            }
                            if let Some(thumb_sheet) = pd.thumb_sheet {
                                res.ts_cols = thumb_sheet.cols.to_string();
                                res.ts_rows = thumb_sheet.rows.to_string();
                                res.thumbs_complete = true;
                            }
                        }
                    }

                    if (expect_thumbnail == res.thumbs_complete) && (expect_transcode == res.transcode_complete) {
                        break 'waitloop;
                    } else {
                        tracing::info!("Not done yet: transcode_complete = {} (expected: {}), thumbs_complete = {} (expected: {})...",
                            res.transcode_complete, expect_transcode,
                            res.thumbs_complete, expect_thumbnail);

                    }
                },

                something_else => {
                    tracing::info!("Got UNEXPECTED (not necessarily a bug) message: {:?}", something_else);
                },
            }
            thread::sleep(Duration::from_secs_f32(0.1));
        }

        tracing::info!("Transcode complete: {} (expeted: {}), thumbs complete: {} (expected: {})",
            res.transcode_complete, expect_transcode,
            res.thumbs_complete, expect_thumbnail);

        if let Some((data_dir, input_filename)) = check_file_outputs {
            let vid_dir = data_dir.join("videos").join(vid);
            let thumb_dir = vid_dir.join("thumbs");

            assert!(vid_dir.join("orig").join(input_filename).is_file());
            if expect_transcode {
                assert!(vid_dir.join("video.mp4").is_symlink());
                assert!(vid_dir.join("stdout.txt").is_file());
                assert!(vid_dir.join("stderr.txt").is_file());
            }

            if expect_thumbnail {
                assert!(thumb_dir.join("thumb.webp").is_file());
            }
            if expect_thumbsheet {
                assert!(u32::from_str(&res.ts_cols).ok().unwrap() > 0);
                assert!(u32::from_str(&res.ts_rows).ok().unwrap() > 0);
                assert!(thumb_dir.join(format!("sheet-{}x{}.webp", res.ts_cols, res.ts_rows)).is_file());
            }
            if expect_thumbnail || expect_thumbsheet {
                assert!(thumb_dir.join("stdout.txt").is_file());
                assert!(thumb_dir.join("stderr.txt").is_file());
            }
        }

        res
    }


    async fn wait_for_any_client_msg(mut ws: &mut crate::api_server::test_utils::WsClient)
    {
        for _ in 0..(60*2*10)
        {
            match crate::api_server::test_utils::try_get_parsed::<ServerToClientCmd>(&mut ws).await.map(|c| c.cmd).flatten() {
                Some(x) => {
                    tracing::info!("Got message: {:?}", x);
                    return;
                },
                None => {
                    thread::sleep(Duration::from_millis(50));
                },
            };
        }
    }


    #[test]
    #[serial]
    #[traced_test]
    #[cfg(feature = "include_slow_tests")]
    fn test_video_mov_ingest_and_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 500_000, None, None]
            // Copy test file to incoming dir
            let mov_file = "NASA_Red_Lettuce_excerpt.mov";

            let dangerous_name = "  -fake-arg name; \"and some more'.txt 你 .mov";
            data_dir.copy_from("src/tests/assets/", &[mov_file]).unwrap();
            std::fs::rename(data_dir.join(mov_file), incoming_dir.join(dangerous_name)).unwrap();

            let wait_res = wait_for_reports(&mut ws, true, true, true, Some((data_dir.path().into(), dangerous_name.into()))).await;

            assert!(wait_res.transcode_complete, "Transcode did not complete / was not marked done");
            assert!(wait_res.got_progress_report);
        }
        Ok(())
    }


    #[test]
    #[serial]
    #[traced_test]
    #[cfg(feature = "include_slow_tests")]
    fn test_video_12bit_dnxhr_alpha_ingest_and_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 500_000, None, None]
            // Copy test file to incoming dir
            let mov_file = "alpha-test_dnxhr-444-12bit-dnxhr.mov";

            data_dir.copy_from("src/tests/assets/", &[mov_file]).unwrap();
            std::fs::rename(data_dir.join(mov_file), incoming_dir.join(mov_file)).unwrap();

            let wait_res = wait_for_reports(&mut ws, true, true, true, Some((data_dir.path().into(), mov_file.into()))).await;

            assert!(wait_res.transcode_complete, "Transcode did not complete / was not marked done");
            assert!(wait_res.got_progress_report);
        }
        Ok(())
    }


    #[test]
    #[serial]
    #[traced_test]
    #[cfg(feature = "include_slow_tests")]
    fn test_audio_ingest_and_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 500_000, None, None]
            // Copy test file to incoming dir
            let audio_file_name = "drunkards-special-short-mono.wav";
            data_dir.copy_from("src/tests/assets/", &[audio_file_name]).unwrap();
            std::fs::rename(data_dir.join(audio_file_name), incoming_dir.join(audio_file_name)).unwrap();

            let wait_res = wait_for_reports(&mut ws, true, false, false, Some((data_dir.path().into(), audio_file_name.into()))).await;    // No thumbnail for audio
        }
        Ok(())
    }


    #[test]
    #[serial]
    #[traced_test]
    fn test_image_ingest_and_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 500_000, None, None]
            let image_file_name = "NASA-48410_PIA25967_-_MAV_Test.jpeg";
            data_dir.copy_from("src/tests/assets/", &[image_file_name]).unwrap();
            std::fs::rename(data_dir.join(image_file_name), incoming_dir.join(image_file_name)).unwrap();

            let wait_res = wait_for_reports(&mut ws, true, true, false, Some((data_dir.path().into(), image_file_name.into()))).await;
        }
        Ok(())
    }



    #[test]
    #[serial]
    #[traced_test]
    fn test_existing_v056_migrate_and_image_ingest() -> anyhow::Result<()>
    {
        let (_db, temp_dir, _videos, _comments) = crate::database::tests::make_test_db();

        // Overwrite the test DB with one from assets dir, for migration testing on existing DB
        let db_file = temp_dir.path().join("clapshot.sqlite");
        std::fs::copy("src/tests/assets/databases/clapshot-migration-test-1_v056.sqlite", &db_file)
            .expect("Failed to copy test DB for migration test");

        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 500_000, None, Some(temp_dir)]
            let image_file_name = "NASA-48410_PIA25967_-_MAV_Test.jpeg";
            data_dir.copy_from("src/tests/assets/", &[image_file_name]).unwrap();
            std::fs::rename(data_dir.join(image_file_name), incoming_dir.join(image_file_name)).unwrap();
            let wait_res = wait_for_reports(&mut ws, true, true, false, Some((data_dir.path().into(), image_file_name.into()))).await;
        }
        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_organizer_existing_v056_migrate() -> anyhow::Result<()>
    {
        // This supplements the other v056_migrate test, by testing with Organizer too.
        match std::env::var("TEST_ORG_CMD").ok()
        {
            Some(org_cmd) => {
                let (_db, temp_dir, _videos, _comments) = crate::database::tests::make_test_db();
                // Overwrite the test DB with one from assets dir, for migration testing on existing DB
                let db_file = temp_dir.path().join("clapshot.sqlite");
                std::fs::copy("src/tests/assets/databases/clapshot-migration-test-1_v056.sqlite", &db_file).expect("Failed to copy test DB for migration test");
                cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 500_000, Some(org_cmd), Some(temp_dir)]
                    // If we get any client messages, Organizer migration was successful and API server was started
                    wait_for_any_client_msg(&mut ws).await;
                }
            },
            None => {
                tracing::info!("Organizer cmd not specified, skipping organizer test");
            }
        }
        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_organizer_existing_v061_migrate() -> anyhow::Result<()>
    {
        match std::env::var("TEST_ORG_CMD").ok()
        {
            Some(org_cmd) => {
                let (_db, temp_dir, _videos, _comments) = crate::database::tests::make_test_db();
                // Overwrite the test DB with one from assets dir, for migration testing on existing DB
                let db_file = temp_dir.path().join("clapshot.sqlite");
                std::fs::copy("src/tests/assets/databases/clapshot-migration-test-2_v061.sqlite", &db_file).expect("Failed to copy test DB for migration test");
                cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, 500_000, Some(org_cmd), Some(temp_dir)]
                    // If we get any client messages, Organizer migration was successful and API server was started
                    wait_for_any_client_msg(&mut ws).await;
                }
            },
            None => {
                tracing::info!("Organizer cmd not specified, skipping organizer test");
            }
        }
        Ok(())
    }




    #[test]
    #[serial]
    #[traced_test]
    fn test_organizer_run_organizer_tests() -> anyhow::Result<()>
    {
        // Environment variable TEST_ORG_CMD can be used to specify a command
        // to start organizer. If not specified, the test will be skipped.
        match std::env::var("TEST_ORG_CMD").ok()
        {
            Some(cmd) => {

                // `cargo test` captures stdout/stderr, so we can't list the test to console,
                // put them in a log file instead. Open & truncate here, so it's empty if
                // listing fails.
                let log_path = std::env::var("TEST_ORG_LOG").unwrap_or("organizer_tests.log".into());
                let log = Arc::new(Mutex::new(std::io::BufWriter::new(
                    std::fs::File::create(&log_path).expect(format!("Failed to create log file '{}'", &log_path).as_str()))));

                fn write_log<W: Write + Send>(writer: &Arc<Mutex<W>>, s: &str) {
                    let mut writer = writer.lock().unwrap();
                    writeln!(writer, "{}", s).unwrap();
                    writer.flush().ok();
                    println!("{}", s);
                }

                let test_results: Arc<Mutex<Vec<(String, org::RunTestResponse)>>> = Arc::new(Mutex::new(Vec::new()));

                // Connect to organizer and list its test names
                write_log(&log, "    Retrieving organizer tests...");
                let test_names: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
                {
                    let test_names = test_names.clone();
                    cs_main_test! {[_ws, data_dir, incoming_dir, org_conn, 500_000, Some(cmd.clone()), None]
                        match org_conn {
                            Some(mut org_conn) => {
                                match org_conn.list_tests(proto::Empty {}).await {
                                    Ok(res) => { test_names.lock().unwrap().extend(res.into_inner().test_names); },
                                    Err(e) => match e.code() {
                                        tonic::Code::Unimplemented | tonic::Code::NotFound => {} ,
                                        _ => {
                                            panic!("Organizer list_tests failed: {:?}", e);
                                        },
                                    }};
                            },
                            None => {
                                panic!("Organizer connection failed!");
                            }
                        }
                    }
                }

                println!("\n\n^^^ (that was just a call listing organizer tests, now running them...) ^^^");

                // Call gRPC run_test() for each test name. Store results in test_results.
                let test_names: Vec<String> = test_names.lock().unwrap().iter().map(|s| s.clone()).collect();
                write_log(&log, format!("    Running {} organizer tests", test_names.len()).as_str());

                for (i, test_name) in test_names.iter().enumerate()
                {
                    println!("\n\n\n------------ Running organizer test {}/{}: '{}'... ------------\n\n\n", i+1, test_names.len()+1, test_name);

                    let (_db, temp_dir, _videos, _comments) = crate::database::tests::make_test_db();
                    let test_results = test_results.clone();
                    let log = log.clone();

                    cs_main_test! {[_ws, data_dir, incoming_dir, org_conn, 500_000, Some(cmd.clone()), Some(temp_dir)]
                        match org_conn {
                            Some(mut org_conn) => {
                                match org_conn.run_test(org::RunTestRequest { test_name: test_name.clone() }).await {
                                    Ok(res) => {
                                        let mut res = res.into_inner().clone();
                                        res.error = res.error.as_ref().filter(|s| !s.is_empty()).cloned(); // Remove empty error strings (assume they are not errors)
                                        write_log(&log, format!("    Org test '{}' ... {}",
                                                test_name,
                                                if res.error.is_none() { "ok" } else { "FAILED" }
                                            ).as_str());
                                        test_results.lock().unwrap().push((test_name.clone(), res));
                                    },
                                    Err(e) => {
                                        write_log(&log, format!("    Org test '{}' ... FAILED", test_name).as_str());
                                        test_results.lock().unwrap().push((test_name.clone(), RunTestResponse {
                                            output: "gRPC call to org.run_test() failed".to_string(),
                                            error: Some(format!("{:?}", e)),}));
                                    }
                                }
                            }
                            None => { panic!("Organizer connection failed"); }
                        }
                    }
                }

                // Write test results to log file and print to console, mimicking cargo test output
                let test_results = test_results.lock().unwrap();
                for (test_name, res) in test_results.iter()
                {
                    if let Some(err) = &res.error {
                        write_log(&log, format!("\n\n").as_str());
                        write_log(&log, format!("==================== FAILED ORG TEST: '{}' ====================", test_name).as_str());
                        write_log(&log, format!("(NOTE! For Clapshot Server -captured logs, see the cargo test output for integration_test::test_organizer!)").as_str());
                        write_log(&log, format!("\n---------------- RunTestResponse.output ----------------").as_str());
                        write_log(&log, format!("{}", res.output).as_str());
                        write_log(&log, format!("\n---------------- RunTestResponse.error ----------------").as_str());
                        write_log(&log, format!("{}", err).as_str());
                        write_log(&log, format!("\n\n").as_str());
                    }
                }
                if test_results.iter().any(|(_, res)| res.error.is_some()) {
                    write_log(&log, format!("### Some organizer tests failed ###").as_str());
                    panic!("Some organizer tests failed, output also logged into '{}'", log_path);
                }
            },
            None => {
                tracing::info!("Organizer cmd not specified, skipping organizer test");
            }
        }
        Ok(())
    }

}
