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
    use std::{path::{Path, PathBuf}, str::FromStr};
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
    use crate::api_server::validate_org_http_headers_regex;
    use crate::storage::StorageBackend;

    use crate::database::schema::media_files::{thumb_sheet_cols, thumb_sheet_rows};
    use crate::{expect_client_cmd, send_server_cmd};
    use crate::grpc::grpc_client::prepare_organizer;
    use crate::video_pipeline::{metadata_reader, IncomingFile, IngestUsernameFrom};
    use crate::api_server::test_utils::{connect_client_ws, open_media_file, write};
    use lib_clapshot_grpc::{GrpcBindAddr, proto};
    use lib_clapshot_grpc::proto::client::ServerToClientCmd;
    use lib_clapshot_grpc::proto::client::server_to_client_cmd as s2c;
    use pbjson_types;

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
            transcode_preference: crate::video_pipeline::TranscodePreference::Auto,
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
        // 8-param variant: default storage, no ws_user_override
        ([$ws:ident, $data_dir:ident, $incoming_dir:ident, $org_conn:ident, $url_base:ident, $bitrate:expr, $org_cmd:expr, $custom_assertfs:expr, $ingest_username_from:expr] $($body:tt)*) => {
            cs_main_test!(@impl [$ws, $data_dir, $incoming_dir, $org_conn, $url_base, $bitrate, $org_cmd, $custom_assertfs, $ingest_username_from, None,
                |media_root: std::path::PathBuf, url_base: &str| crate::storage::StorageBackend::local(media_root, url_base)] $($body)*)
        };
        // 9-param variant: default storage, with ws_user_override
        ([$ws:ident, $data_dir:ident, $incoming_dir:ident, $org_conn:ident, $url_base:ident, $bitrate:expr, $org_cmd:expr, $custom_assertfs:expr, $ingest_username_from:expr, $ws_user_override:expr] $($body:tt)*) => {
            cs_main_test!(@impl [$ws, $data_dir, $incoming_dir, $org_conn, $url_base, $bitrate, $org_cmd, $custom_assertfs, $ingest_username_from, $ws_user_override,
                |media_root: std::path::PathBuf, url_base: &str| crate::storage::StorageBackend::local(media_root, url_base)] $($body)*)
        };
        // 10-param variant: custom storage factory (receives media_root and url_base)
        ([$ws:ident, $data_dir:ident, $incoming_dir:ident, $org_conn:ident, $url_base:ident, $bitrate:expr, $org_cmd:expr, $custom_assertfs:expr, $ingest_username_from:expr, $ws_user_override:expr, $storage_factory:expr] $($body:tt)*) => {
            cs_main_test!(@impl [$ws, $data_dir, $incoming_dir, $org_conn, $url_base, $bitrate, $org_cmd, $custom_assertfs, $ingest_username_from, $ws_user_override,
                |media_root: std::path::PathBuf, url_base: &str| { let f = $storage_factory; f(media_root, url_base) }] $($body)*)
        };
        // Single implementation - storage_factory takes (media_root, url_base)
        (@impl [$ws:ident, $data_dir:ident, $incoming_dir:ident, $org_conn:ident, $url_base:ident, $bitrate:expr, $org_cmd:expr, $custom_assertfs:expr, $ingest_username_from:expr, $ws_user_override:expr, $storage_factory:expr] $($body:tt)*) => {
            {
                let $data_dir = $custom_assertfs.unwrap_or(assert_fs::TempDir::new().unwrap());
                let $incoming_dir = $data_dir.join("incoming");
                std::fs::create_dir_all($incoming_dir.as_path()).unwrap();

                // Run server
                let port = portpicker::pick_unused_port().expect("No TCP ports free");
                let $url_base = format!("http://127.0.0.1:{}", port);
                let ws_url = format!("{}/api/ws", &$url_base.replace("http", "ws"));
                let target_bitrate = $bitrate;
                let regex = validate_org_http_headers_regex("^X[-_]REMOTE[-_]").unwrap();

                let grpc_server_bind = crate::grpc::grpc_server::make_grpc_server_bind(&None, &$data_dir)?;
                let (org_uri, _org_hdl) = prepare_organizer(&None, &$org_cmd, tracing::Level::DEBUG, false, &$data_dir.path())?;

                let terminate_flag = Arc::new(AtomicBool::new(false));

                let th = {
                    let poll_interval = 0.1;
                    let data_dir = $data_dir.path().to_path_buf();
                    let url_base_for_storage = $url_base.clone();
                    let org_uri = org_uri.clone();
                    let media_root = data_dir.join("videos");
                    let storage = { let f = $storage_factory; f(media_root, &url_base_for_storage) };
                    let tf = terminate_flag.clone();
                    thread::spawn(move || {
                        let mut clapshot = crate::ClapshotInit::init_and_spawn_workers(data_dir, true, url_base_for_storage, vec![], "127.0.0.1".into(), port, org_uri.clone(), grpc_server_bind, 4, target_bitrate, poll_interval, "anonymous".to_string(), poll_interval*5.0, $ingest_username_from, "scripts/clapshot-transcode".to_string(), "scripts/clapshot-thumbnail".to_string(), "scripts/clapshot-transcode-decision".to_string(), regex, storage, tf)?;
                        clapshot.wait_for_termination()
                })};

                assert!(wait_for_healthy(&$url_base), "Server API never became healthy");

                tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
                    // Connect client
                    let cur_process_user = whoami::username();
                    let ws_user = $ws_user_override.unwrap_or(cur_process_user);
                    let mut $ws = connect_client_ws(&ws_url, &ws_user).await;
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
        };
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_video_ingest_no_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 2500_000, None, None, IngestUsernameFrom::FileOwner]
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
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, None, IngestUsernameFrom::FileOwner]
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
        pub media_id: String,
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
            media_id: String::new(),
            transcode_complete: false, thumbs_complete: false,
            got_progress_report: false, got_transcode_report: false, got_thumbnail_report: false,
            ts_cols: String::new(), ts_rows: String::new(),
        };

        const WAIT_AFTER_REPORTS_TIMEOUT_SECS: u32 = 5;

        // Wait for file to be processed
        thread::sleep(Duration::from_secs_f32(0.5));
        let msg = expect_user_msg(&mut ws, proto::user_message::Type::MediaFileAdded).await;    // notification to client (with upload folder info etc)
        let vid = msg.refs.unwrap().media_file_id.unwrap();
        res.media_id = vid.clone();

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
                        // Got progress report? Count explicit Progress topics as well as any
                        // message carrying a non-zero progress value (e.g. the final "Media transcoded." update).
                        res.got_progress_report |= m.msgs.iter().any(|msg| {
                            msg.r#type == proto::user_message::Type::Progress as i32
                                || msg.progress.map(|p| p > 0.0).unwrap_or(false)
                        });

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
                    // Give any in-flight progress messages a moment to arrive before we request the page.
                    thread::sleep(Duration::from_millis(200));
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
                    res.got_progress_report |= m.msgs.iter().any(|msg| {
                        msg.r#type == proto::user_message::Type::Progress as i32
                            || msg.progress.map(|p| p > 0.0).unwrap_or(false)
                    });
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
                assert!(vid_dir.join("transcode.log").is_file());
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
                assert!(thumb_dir.join("thumbnail.log").is_file());
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
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, None, IngestUsernameFrom::FileOwner]
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
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, None, IngestUsernameFrom::FileOwner]
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
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, None, IngestUsernameFrom::FileOwner]
            // Copy test file to incoming dir
            let audio_file_name = "drunkards-special-short-mono.wav";
            data_dir.copy_from("src/tests/assets/", &[audio_file_name]).unwrap();
            std::fs::rename(data_dir.join(audio_file_name), incoming_dir.join(audio_file_name)).unwrap();

            let wait_res = wait_for_reports(&mut ws, true, false, false, Some((data_dir.path().into(), audio_file_name.into()))).await;    // No thumbnail for audio

            // Check that waveform video file was created for audio transcoding
            let videos_dir = data_dir.join("videos");
            let mut found_video = false;
            if let Ok(entries) = std::fs::read_dir(&videos_dir) {
                for entry in entries.flatten() {
                    let video_mp4 = entry.path().join("video.mp4");
                    if video_mp4.exists() {
                        found_video = true;
                        break;
                    }
                }
            }
            assert!(found_video, "Audio transcoding should create waveform video file");
        }
        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    #[cfg(feature = "include_slow_tests")]
    fn test_mp3_full_integration() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, None, IngestUsernameFrom::FileOwner]
            // Copy the MP3 file to incoming dir and test full integration
            let audio_file_name = "Apollo11_countdown.mp3";
            data_dir.copy_from("src/tests/assets/", &[audio_file_name]).unwrap();
            std::fs::rename(data_dir.join(audio_file_name), incoming_dir.join(audio_file_name)).unwrap();

            println!("DEBUG: Testing MP3 file integration: {}", audio_file_name);

            // This should work correctly with audio file processing
            let wait_res = wait_for_reports(&mut ws, true, false, false, Some((data_dir.path().into(), audio_file_name.into()))).await;    // No thumbnail for audio

            // Check that waveform video file was created for audio transcoding
            let videos_dir = data_dir.join("videos");
            let mut found_video = false;
            if let Ok(entries) = std::fs::read_dir(&videos_dir) {
                for entry in entries.flatten() {
                    let video_mp4 = entry.path().join("video.mp4");
                    if video_mp4.exists() {
                        found_video = true;
                        break;
                    }
                }
            }
            assert!(found_video, "Audio transcoding should create waveform video file");
        }
        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_mp3_metadata_detection() -> anyhow::Result<()>
    {
        use crossbeam_channel;
        use crate::video_pipeline::metadata_reader;
        use crate::video_pipeline::IncomingFile;
        use std::collections::HashMap;

        // Test that MP3 files are correctly detected as Audio
        let test_file = std::path::PathBuf::from("src/tests/assets/Apollo11_countdown.mp3");

        let incoming_file = IncomingFile {
            file_path: test_file.clone(),
            user_id: "test_user".to_string(),
            cookies: HashMap::new(),
            transcode_preference: crate::video_pipeline::TranscodePreference::Auto,
        };

        let (tx, rx) = crossbeam_channel::unbounded();
        let (result_tx, result_rx) = crossbeam_channel::unbounded();

        // Start metadata reader
        std::thread::spawn(move || {
            metadata_reader::run_forever(rx, result_tx, 1);
        });

        // Send the file for processing
        tx.send(incoming_file).unwrap();

        // Get the result
        match result_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(result) => {
                match result {
                    metadata_reader::MetadataResult::Ok(metadata) => {
                        // After the fix, MP3 files should be correctly detected as Audio
                        assert_eq!(format!("{:?}", metadata.media_type), "Audio", "MP3 file should be detected as Audio");

                        // Duration should be reasonable for the test file (~25 seconds)
                        assert!(metadata.duration > rust_decimal::Decimal::from(20), "Duration should be > 20 seconds");
                        assert!(metadata.duration < rust_decimal::Decimal::from(30), "Duration should be < 30 seconds");
                    }
                    metadata_reader::MetadataResult::Err(e) => {
                        panic!("Metadata reading failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                panic!("Timeout waiting for metadata result: {:?}", e);
            }
        }

        Ok(())
    }


    #[test]
    #[serial]
    #[traced_test]
    fn test_image_ingest_and_transcode() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, None, IngestUsernameFrom::FileOwner]
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

        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, Some(temp_dir), IngestUsernameFrom::FileOwner]
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
                cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, Some(org_cmd), Some(temp_dir), IngestUsernameFrom::FileOwner]
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
                cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, Some(org_cmd), Some(temp_dir), IngestUsernameFrom::FileOwner]
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
                    cs_main_test! {[_ws, data_dir, incoming_dir, org_conn, url_base, 500_000, Some(cmd.clone()), None, IngestUsernameFrom::FileOwner]
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
                let mut test_names: Vec<String> = test_names.lock().unwrap().iter().map(|s| s.clone()).collect();

                // Check for TEST_ORG_FILTER environment variable to filter tests
                if let Some(filter) = std::env::var("TEST_ORG_FILTER").ok().filter(|s| !s.is_empty()) {
                    write_log(&log, format!("    Filtering tests with pattern: '{}'", filter).as_str());
                    test_names.retain(|name| name.contains(&filter));
                    if test_names.is_empty() {
                        write_log(&log, format!("    No tests match filter '{}'", filter).as_str());
                        panic!("No organizer tests match the filter '{}'", filter);
                    }
                }

                write_log(&log, format!("    Running {} organizer tests", test_names.len()).as_str());

                for (i, test_name) in test_names.iter().enumerate()
                {
                    println!("\n\n\n------------ Running organizer test {}/{}: '{}'... ------------\n\n\n", i+1, test_names.len()+1, test_name);

                    let (_db, temp_dir, _videos, _comments) = crate::database::tests::make_test_db();
                    let test_results = test_results.clone();
                    let log = log.clone();

                    cs_main_test! {[_ws, data_dir, incoming_dir, org_conn, url_base, 500_000, Some(cmd.clone()), Some(temp_dir), IngestUsernameFrom::FileOwner]
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

    #[test]
    #[serial]
    #[traced_test]
    fn test_ingest_username_from_file_owner() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 2500_000, None, None, IngestUsernameFrom::FileOwner]
            // Copy test file to incoming dir (owned by current user)
            let mp4_file = "60fps-example.mp4";
            data_dir.copy_from("src/tests/assets/", &[mp4_file]).unwrap();
            std::fs::rename(data_dir.join(mp4_file), incoming_dir.join(mp4_file)).unwrap();

            // Wait for file to be processed
            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::MediaFileAdded).await;
            let vid = msg.refs.unwrap().media_file_id.unwrap();

            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;
            let vid2 = msg.refs.unwrap().media_file_id.unwrap();
            assert_eq!(vid, vid2);

            crate::api_server::test_utils::wait_for_thumbnails(&mut ws).await;

            // Open media file and verify the username matches file owner
            let media_file = open_media_file(&mut ws, &vid).await.media_file.unwrap();
            let current_user = whoami::username();
            assert_eq!(media_file.user_id, current_user, "Username should match file owner");
        }
        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_ingest_username_from_folder_name() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 2500_000, None, None, IngestUsernameFrom::FolderName, Some("test_folder_user".to_string())]
            // Create user folder structure with specific test username
            let current_user = whoami::username();
            let username = "test_folder_user".to_string(); // Different from file owner - proves folder extraction works
            let user_dir = incoming_dir.join(&username);
            std::fs::create_dir_all(&user_dir).unwrap();

            // Copy test file to user folder
            let mp4_file = "60fps-example.mp4";
            data_dir.copy_from("src/tests/assets/", &[mp4_file]).unwrap();
            std::fs::rename(data_dir.join(mp4_file), user_dir.join(mp4_file)).unwrap();

            // Wait for file to be processed
            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::MediaFileAdded).await;
            let vid = msg.refs.unwrap().media_file_id.unwrap();

            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;
            let vid2 = msg.refs.unwrap().media_file_id.unwrap();
            assert_eq!(vid, vid2);

            crate::api_server::test_utils::wait_for_thumbnails(&mut ws).await;

            // Open media file and verify the username was extracted from folder name, not file owner
            let media_file = open_media_file(&mut ws, &vid).await.media_file.unwrap();
            assert_eq!(media_file.user_id, username, "Username should match folder name");
            assert_ne!(media_file.user_id, current_user, "Username should NOT match file owner - proves folder extraction worked");
        }
        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_ingest_username_from_folder_name_nested() -> anyhow::Result<()>
    {
        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 2500_000, None, None, IngestUsernameFrom::FolderName, Some("test_nested_user".to_string())]
            // Create folder structure with specific test username
            let current_user = whoami::username();
            let username = "test_nested_user".to_string(); // Different from file owner - proves folder extraction works
            let user_dir = incoming_dir.join(&username);
            std::fs::create_dir_all(&user_dir).unwrap();

            // Copy test file to user folder
            let mp4_file = "60fps-example.mp4";
            data_dir.copy_from("src/tests/assets/", &[mp4_file]).unwrap();
            std::fs::rename(data_dir.join(mp4_file), user_dir.join(mp4_file)).unwrap();

            // Wait for file to be processed
            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::MediaFileAdded).await;
            let vid = msg.refs.unwrap().media_file_id.unwrap();

            thread::sleep(Duration::from_secs_f32(0.5));
            let msg = expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;
            let vid2 = msg.refs.unwrap().media_file_id.unwrap();
            assert_eq!(vid, vid2);

            crate::api_server::test_utils::wait_for_thumbnails(&mut ws).await;

            // Open media file and verify the username was extracted from folder name, not file owner
            let media_file = open_media_file(&mut ws, &vid).await.media_file.unwrap();
            assert_eq!(media_file.user_id, username, "Username should match folder name");
            assert_ne!(media_file.user_id, current_user, "Username should NOT match file owner - proves folder extraction worked");
        }
        Ok(())
    }

    // ==================== S3/MinIO Integration Tests ====================

    const TEST_BUCKET: &str = "clapshot-test";

    /// Helper to manage a temporary MinIO instance for testing.
    /// Spawns MinIO on a free port with a temp data directory.
    /// Automatically cleans up when dropped.
    struct TempMinIO {
        process: std::process::Child,
        endpoint: String,
        port: u16,
        _data_dir: assert_fs::TempDir,
    }

    impl TempMinIO {
        /// Start a new MinIO instance. Returns None if `minio` is not in PATH.
        fn start() -> Option<Self> {
            // Check if minio binary is available
            if std::process::Command::new("minio")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_err()
            {
                tracing::warn!("MinIO binary not found in PATH - skipping S3 tests");
                return None;
            }

            let port = portpicker::pick_unused_port().expect("No TCP ports free");
            let console_port = portpicker::pick_unused_port().expect("No TCP ports free for console");
            let data_dir = assert_fs::TempDir::new().expect("Failed to create temp dir for MinIO");

            tracing::info!("Starting MinIO on port {} with data dir {:?}", port, data_dir.path());

            let process = std::process::Command::new("minio")
                .arg("server")
                .arg(data_dir.path())
                .arg("--address")
                .arg(format!("127.0.0.1:{}", port))
                .arg("--console-address")
                .arg(format!("127.0.0.1:{}", console_port))
                .env("MINIO_ROOT_USER", "minioadmin")
                .env("MINIO_ROOT_PASSWORD", "minioadmin")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("Failed to start MinIO");

            let endpoint = format!("http://127.0.0.1:{}", port);

            // Wait for MinIO to be ready
            let start = std::time::Instant::now();
            let timeout = Duration::from_secs(10);
            loop {
                if start.elapsed() > timeout {
                    tracing::error!("MinIO failed to start within timeout");
                    return None;
                }
                if reqwest::blocking::get(format!("{}/minio/health/live", endpoint))
                    .map(|r| r.status().is_success())
                    .unwrap_or(false)
                {
                    tracing::info!("MinIO is ready on {}", endpoint);
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }

            Some(TempMinIO {
                process,
                endpoint,
                port,
                _data_dir: data_dir,
            })
        }

        /// Create an S3 client for this MinIO instance (for test verification)
        fn s3_client(&self) -> aws_sdk_s3::Client {
            use aws_sdk_s3::config::Region;

            let rt = tokio::runtime::Runtime::new().unwrap();
            let endpoint = self.endpoint.clone();
            rt.block_on(async move {
                let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .endpoint_url(&endpoint)
                    .region(Region::new("us-east-1"))
                    .load()
                    .await;
                let s3_config = aws_sdk_s3::config::Builder::from(&config)
                    .force_path_style(true)
                    .build();
                aws_sdk_s3::Client::from_conf(s3_config)
            })
        }

        /// Create the test bucket
        fn create_bucket(&self) -> anyhow::Result<()> {
            let client = self.s3_client();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                client
                    .create_bucket()
                    .bucket(TEST_BUCKET)
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create bucket: {}", e))?;
                Ok(())
            })
        }

        /// Create a StorageBackend for this MinIO instance
        fn storage_backend(&self, media_root: PathBuf, prefix: &str, url_base: &str) -> anyhow::Result<StorageBackend> {
            StorageBackend::s3(
                media_root,
                TEST_BUCKET.to_string(),
                Some(self.endpoint.clone()),
                prefix.to_string(),
                Some(format!("{}/{}", self.endpoint, TEST_BUCKET)),
                url_base.to_string(),
                Duration::from_secs(3600),
            )
        }

        /// Check if an object exists in S3
        fn object_exists(&self, key: &str) -> bool {
            let client = self.s3_client();
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                client
                    .head_object()
                    .bucket(TEST_BUCKET)
                    .key(key)
                    .send()
                    .await
                    .is_ok()
            })
        }

        /// List all objects under a prefix (blocking version for non-async tests)
        fn list_objects(&self, prefix: &str) -> Vec<String> {
            let client = self.s3_client();
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(Self::list_objects_async(&client, prefix))
        }

        /// List all objects under a prefix (async version for use inside async contexts)
        async fn list_objects_async(client: &aws_sdk_s3::Client, prefix: &str) -> Vec<String> {
            client
                .list_objects_v2()
                .bucket(TEST_BUCKET)
                .prefix(prefix)
                .send()
                .await
                .map(|r| r.contents().iter().filter_map(|o| o.key().map(String::from)).collect())
                .unwrap_or_default()
        }

        /// Set up AWS env vars for SDK credential chain
        fn setup_env_vars() {
            std::env::set_var("AWS_ACCESS_KEY_ID", "minioadmin");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "minioadmin");
            std::env::set_var("AWS_REGION", "us-east-1");
        }

        /// Set up a complete test environment. Returns (storage, data_dir, prefix).
        fn setup_test(&self) -> anyhow::Result<(StorageBackend, assert_fs::TempDir, String)> {
            TempMinIO::setup_env_vars();
            self.create_bucket()?;

            let data_dir = assert_fs::TempDir::new()?;
            let media_root = data_dir.path().join("videos");
            std::fs::create_dir_all(&media_root)?;

            let prefix = format!("test-{}", uuid::Uuid::new_v4());
            let url_base = "http://127.0.0.1:8080".to_string();
            let storage = self.storage_backend(media_root, &prefix, &url_base)?;

            Ok((storage, data_dir, prefix))
        }
    }

    impl Drop for TempMinIO {
        fn drop(&mut self) {
            tracing::info!("Stopping MinIO on port {}", self.port);
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }

    /// Tests S3 upload for both small files (simple PUT) and large files (multipart).
    /// Also verifies progress callback and media_base_url.
    #[test]
    #[serial]
    #[traced_test]
    fn test_s3_storage_upload() -> anyhow::Result<()> {
        let minio = match TempMinIO::start() {
            Some(m) => m,
            None => return Ok(()), // Skip if MinIO not available
        };
        let (storage, data_dir, prefix) = minio.setup_test()?;
        let media_root = data_dir.path().join("videos");

        // Verify media_base_url is correct
        let expected_url = format!("{}/{}/{}", minio.endpoint, TEST_BUCKET, prefix);
        assert_eq!(storage.media_base_url(), expected_url);

        // Test 1: Small file upload (simple PUT, below 5MB threshold)
        let small_dir = media_root.join("small-file");
        std::fs::create_dir_all(&small_dir)?;
        let small_file = small_dir.join("small.mp4");
        std::fs::write(&small_file, b"small test content")?;
        storage.upload_local_path(&small_file)?;
        assert!(minio.object_exists(&format!("{}/small-file/small.mp4", prefix)),
            "Small file should exist in S3");

        // Test 2: Large file upload (multipart, 10MB > 5MB threshold) with progress
        let large_dir = media_root.join("large-file");
        std::fs::create_dir_all(&large_dir)?;
        let large_file = large_dir.join("large.mp4");
        std::fs::write(&large_file, vec![0u8; 10 * 1024 * 1024])?;

        let progress_values = Arc::new(std::sync::Mutex::new(Vec::new()));
        let pv = progress_values.clone();
        let progress_cb: crate::storage::ProgressCallback = Arc::new(move |p| {
            pv.lock().unwrap().push(p);
        });
        storage.upload_with_progress(&large_file, Some(progress_cb))?;

        let progress = progress_values.lock().unwrap();
        assert!(!progress.is_empty(), "Progress should have been reported");
        assert!((progress.last().unwrap() - 1.0).abs() < 0.001, "Final progress should be ~1.0");
        assert!(minio.object_exists(&format!("{}/large-file/large.mp4", prefix)),
            "Large file should exist in S3");

        // Test 3: Presigned URL generation
        let rt = tokio::runtime::Runtime::new()?;
        let presigned = rt.block_on(storage.presigned_url("small-file", "small.mp4"))?;
        assert!(presigned.contains("X-Amz-Signature"), "Presigned URL should contain signature");
        assert!(presigned.contains(&format!("{}/small-file/small.mp4", prefix)),
            "Presigned URL should contain the S3 key");

        Ok(())
    }

    /// Full E2E test: ingest video → transcode → upload to S3
    #[test]
    #[serial]
    #[traced_test]
    #[cfg(feature = "include_slow_tests")]
    fn test_s3_video_ingest_transcode_upload() -> anyhow::Result<()> {
        TempMinIO::setup_env_vars();
        let minio = match TempMinIO::start() {
            Some(m) => m,
            None => return Ok(()),
        };
        minio.create_bucket()?;

        let test_prefix = format!("test-e2e-{}", uuid::Uuid::new_v4());
        let minio_endpoint = minio.endpoint.clone();
        let prefix_clone = test_prefix.clone();

        // Get S3 client before entering async context (avoids nested runtime)
        let s3_client = minio.s3_client();

        let storage_factory = move |media_root: PathBuf, url_base: &str| -> StorageBackend {
            StorageBackend::s3(
                media_root, TEST_BUCKET.to_string(), Some(minio_endpoint.clone()),
                prefix_clone.clone(), Some(format!("{}/{}", minio_endpoint, TEST_BUCKET)),
                url_base.to_string(), Duration::from_secs(3600),
            ).expect("Failed to create S3 storage backend")
        };

        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, None, IngestUsernameFrom::FileOwner, None, storage_factory]
            // Ingest test video
            let video_file_name = "NASA_Red_Lettuce_excerpt.mov";
            data_dir.copy_from("src/tests/assets/", &[video_file_name]).unwrap();
            std::fs::rename(data_dir.join(video_file_name), incoming_dir.join(video_file_name)).unwrap();

            // Wait for processing (transcode + thumbnails)
            let wait_res = wait_for_reports(&mut ws, true, true, true, None).await;
            assert!(wait_res.transcode_complete, "Transcode did not complete");
            assert!(wait_res.thumbs_complete, "Thumbnails did not complete");

            // Give S3 upload time to finish
            thread::sleep(Duration::from_secs(2));

            // Verify files in S3 (use async version to avoid nested runtime)
            let objects = TempMinIO::list_objects_async(&s3_client, &format!("{}/{}/", test_prefix, wait_res.media_id)).await;
            tracing::info!("S3 objects: {:?}", objects);

            assert!(objects.iter().any(|k| k.contains("video.mp4")), "Transcoded video missing: {:?}", objects);
            assert!(objects.iter().any(|k| k.contains("/thumbs/")), "Thumbnails missing: {:?}", objects);
        }

        Ok(())
    }

    /// E2E test that the /api/media redirect endpoint returns a presigned S3 URL.
    #[test]
    #[serial]
    #[traced_test]
    #[cfg(feature = "include_slow_tests")]
    fn test_s3_media_redirect_endpoint() -> anyhow::Result<()> {
        TempMinIO::setup_env_vars();
        let minio = match TempMinIO::start() {
            Some(m) => m,
            None => return Ok(()),
        };
        minio.create_bucket()?;

        let test_prefix = format!("test-redirect-{}", uuid::Uuid::new_v4());
        let minio_endpoint = minio.endpoint.clone();
        let prefix_clone = test_prefix.clone();

        let storage_factory = move |media_root: PathBuf, url_base: &str| -> StorageBackend {
            StorageBackend::s3(
                media_root, TEST_BUCKET.to_string(), Some(minio_endpoint.clone()),
                prefix_clone.clone(), Some(format!("{}/{}", minio_endpoint, TEST_BUCKET)),
                url_base.to_string(), Duration::from_secs(3600),
            ).expect("Failed to create S3 storage backend")
        };

        cs_main_test! {[ws, data_dir, incoming_dir, _org_conn, url_base, 500_000, None, None, IngestUsernameFrom::FileOwner, None, storage_factory]
            // Ingest test video
            let video_file_name = "NASA_Red_Lettuce_excerpt.mov";
            data_dir.copy_from("src/tests/assets/", &[video_file_name]).unwrap();
            std::fs::rename(data_dir.join(video_file_name), incoming_dir.join(video_file_name)).unwrap();

            let wait_res = wait_for_reports(&mut ws, true, true, true, None).await;
            assert!(wait_res.transcode_complete, "Transcode did not complete");
            assert!(wait_res.thumbs_complete, "Thumbnails did not complete");

            // Give S3 upload time to finish
            thread::sleep(Duration::from_secs(2));

            // Request the redirect endpoint
            let redirect_url = format!("{}/api/media/{}/video.mp4", url_base, wait_res.media_id);
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Failed to build reqwest client");
            let resp = client
                .get(&redirect_url)
                .header("X-Remote-User-Id", whoami::username())
                .send()
                .await
                .expect("Failed to send redirect request");

            assert_eq!(resp.status(), 302, "Expected redirect response");
            let location = resp.headers()
                .get("Location")
                .and_then(|v| v.to_str().ok())
                .expect("Missing Location header");
            assert!(location.contains("X-Amz-Signature"), "Location should be a presigned S3 URL");
            assert!(location.contains(&format!("{}/{}/video.mp4", test_prefix, wait_res.media_id)),
                "Location should contain the correct S3 key");
        }

        Ok(())
    }

    #[test]
    fn test_s3_storage_needs_remote_upload() -> anyhow::Result<()> {
        let data_dir = assert_fs::TempDir::new()?;
        let media_root = data_dir.path().join("videos");

        // Local storage should not need remote upload
        let local_storage = StorageBackend::local(media_root.clone(), "http://localhost:8080");
        assert!(!local_storage.needs_remote_upload());

        Ok(())
    }

    #[test]
    fn test_storage_media_url_local() {
        let storage = StorageBackend::local(PathBuf::from("/tmp/videos"), "http://localhost:8080");
        assert_eq!(storage.media_url("abc123/video.mp4"), "http://localhost:8080/videos/abc123/video.mp4");
        assert_eq!(storage.media_url("/abc123/thumbs/thumb.webp"), "http://localhost:8080/videos/abc123/thumbs/thumb.webp");
    }

    #[test]
    fn test_storage_media_url_s3() {
        let storage = StorageBackend::s3(
            PathBuf::from("/tmp/videos"),
            "test-bucket".to_string(),
            Some("http://minio.example.com".to_string()),
            "videos".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        ).expect("Failed to create S3 storage");

        assert_eq!(storage.media_url("abc123/video.mp4"), "http://localhost:8080/api/media/abc123/video.mp4");
    }

    #[test]
    fn test_storage_presigned_url_local_errors() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage = StorageBackend::local(PathBuf::from("/tmp/videos"), "http://localhost:8080");
        let res = rt.block_on(storage.presigned_url("abc123", "video.mp4"));
        assert!(res.is_err());
    }

    #[test]
    fn test_subtitle_from_proto3_strips_query_params() {
        let subtitle = proto::Subtitle {
            id: "42".to_string(),
            media_file_id: "abc123".to_string(),
            title: "Test".to_string(),
            language_code: "en".to_string(),
            playback_url: "http://localhost:8080/api/media/abc123/subs/foo.vtt?X-Amz-Signature=abc".to_string(),
            orig_url: "http://localhost:8080/api/media/abc123/subs/orig/bar.srt".to_string(),
            orig_filename: "bar.srt".to_string(),
            added_time: Some(pbjson_types::Timestamp::from(chrono::Utc::now())),
            time_offset: 0.0,
        };

        let model = crate::database::models::Subtitle::from_proto3(&subtitle).expect("from_proto3 failed");
        assert_eq!(model.filename, Some("foo.vtt".to_string()));
    }

    #[test]
    fn test_storage_media_base_url() {
        let local = StorageBackend::local(PathBuf::from("/tmp/videos"), "http://localhost:8080");
        assert_eq!(local.media_base_url(), "http://localhost:8080/videos");

        let s3 = StorageBackend::s3(
            PathBuf::from("/tmp/videos"),
            "test-bucket".to_string(),
            Some("http://minio.example.com".to_string()),
            "videos".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        ).expect("Failed to create S3 storage");
        assert_eq!(s3.media_base_url(), "http://minio.example.com/test-bucket/videos");
    }

    #[test]
    fn test_storage_upload_local_noop() {
        let storage = StorageBackend::local(PathBuf::from("/tmp/videos"), "http://localhost:8080");
        let progress_called = Arc::new(std::sync::Mutex::new(false));
        let pc = progress_called.clone();
        let cb: crate::storage::ProgressCallback = Arc::new(move |p| {
            assert!((p - 1.0).abs() < f32::EPSILON);
            *pc.lock().unwrap() = true;
        });
        storage.upload_with_progress(Path::new("/tmp/videos/foo.mp4"), Some(cb)).expect("local upload should be a no-op");
        assert!(*progress_called.lock().unwrap(), "progress callback should have been invoked");
    }

    #[test]
    fn test_storage_upload_if_exists_local_noop() {
        let storage = StorageBackend::local(PathBuf::from("/tmp/videos"), "http://localhost:8080");
        // Should not panic or error even for a missing path on local backend.
        storage.upload_if_exists(Path::new("/tmp/videos/does-not-exist.mp4"));
    }

    #[test]
    fn test_storage_upload_required_local_noop() {
        let storage = StorageBackend::local(PathBuf::from("/tmp/videos"), "http://localhost:8080");
        storage.upload_required(Path::new("/tmp/videos/foo.mp4")).expect("local upload_required should be a no-op");
    }

    #[test]
    fn test_key_for_path_local() {
        let storage = StorageBackend::local(PathBuf::from("/tmp/videos"), "http://localhost:8080");
        assert_eq!(
            storage.key_for_path(Path::new("/tmp/videos/abc123/video.mp4")).unwrap(),
            "videos/abc123/video.mp4"
        );
        assert!(storage.key_for_path(Path::new("/outside/videos/foo.mp4")).is_err());
    }

    #[test]
    fn test_key_for_path_s3() {
        let storage = StorageBackend::s3(
            PathBuf::from("/tmp/videos"),
            "test-bucket".to_string(),
            Some("http://minio.example.com".to_string()),
            "uploads".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        ).expect("Failed to create S3 storage");
        assert_eq!(
            storage.key_for_path(Path::new("/tmp/videos/abc123/video.mp4")).unwrap(),
            "uploads/abc123/video.mp4"
        );
    }

    #[test]
    fn test_guess_content_type() {
        use std::path::Path;
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.mp4")), "video/mp4");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.mkv")), "video/x-matroska");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.webm")), "video/webm");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.mov")), "video/quicktime");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.webp")), "image/webp");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.png")), "image/png");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.jpg")), "image/jpeg");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.jpeg")), "image/jpeg");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.vtt")), "text/vtt");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.srt")), "application/x-subrip");
        assert_eq!(crate::storage::guess_content_type(Path::new("foo.xyz")), "application/octet-stream");
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_s3_default_public_base_url() {
        // When no endpoint and no public_base_url are provided, should default to AWS S3 URL.
        let storage = StorageBackend::s3(
            PathBuf::from("/tmp/videos"),
            "my-bucket".to_string(),
            None,
            "videos".to_string(),
            None,
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        ).expect("Failed to create AWS S3 storage");

        assert_eq!(storage.media_base_url(), "https://my-bucket.s3.amazonaws.com/videos");
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_s3_upload_empty_file() -> anyhow::Result<()> {
        let minio = match TempMinIO::start() {
            Some(m) => m,
            None => return Ok(()),
        };
        let (storage, data_dir, prefix) = minio.setup_test()?;
        let media_root = data_dir.path().join("videos");

        let dir = media_root.join("empty-file");
        std::fs::create_dir_all(&dir)?;
        let empty_file = dir.join("empty.mp4");
        std::fs::write(&empty_file, b"")?;

        let progress_called = Arc::new(std::sync::Mutex::new(false));
        let pc = progress_called.clone();
        let cb: crate::storage::ProgressCallback = Arc::new(move |p| {
            assert!((p - 1.0).abs() < f32::EPSILON);
            *pc.lock().unwrap() = true;
        });
        storage.upload_with_progress(&empty_file, Some(cb))?;

        assert!(*progress_called.lock().unwrap(), "progress callback should have been invoked for empty file");
        assert!(minio.object_exists(&format!("{}/empty-file/empty.mp4", prefix)),
            "Empty file should exist in S3");

        Ok(())
    }

    #[test]
    #[serial]
    #[traced_test]
    fn test_s3_presigned_url_no_prefix() -> anyhow::Result<()> {
        TempMinIO::setup_env_vars();
        let minio = match TempMinIO::start() {
            Some(m) => m,
            None => return Ok(()),
        };
        let data_dir = assert_fs::TempDir::new()?;
        let media_root = data_dir.path().join("videos");
        std::fs::create_dir_all(&media_root)?;

        minio.create_bucket()?;
        let prefix = format!("test-noprefix-{}", uuid::Uuid::new_v4());
        let storage = StorageBackend::s3(
            media_root,
            TEST_BUCKET.to_string(),
            Some(minio.endpoint.clone()),
            prefix.clone(),
            Some(format!("{}/{}", minio.endpoint, TEST_BUCKET)),
            "http://localhost:8080".to_string(),
            Duration::from_secs(3600),
        )?;

        let rt = tokio::runtime::Runtime::new()?;
        let presigned = rt.block_on(storage.presigned_url("media-id", "video.mp4"))?;
        assert!(presigned.contains("X-Amz-Signature"), "Presigned URL should contain signature");
        assert!(presigned.contains(&format!("{}/media-id/video.mp4", prefix)),
            "Presigned URL should contain the S3 key with prefix");

        Ok(())
    }

}
