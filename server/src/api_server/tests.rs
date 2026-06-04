#![allow(dead_code)]

use std::sync::Arc;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use lib_clapshot_grpc::proto;
use tracing_test::traced_test;
use std::sync::atomic::Ordering::Relaxed;

use reqwest::{multipart, Client};

use crate::database::DbBasicQuery;
use crate::database::error::DBError;
use crate::api_server::{parse_auth_headers, run_api_server_async, validate_org_http_headers_regex, UserMessage, UserMessageTopic};
use crate::api_server::server_state::ServerState;
use crate::database::models::{self};
use crate::database::tests::make_test_db;

use crate::api_server::test_utils::{ApiTestState, expect_msg, expect_no_msg, write, open_media_file, connect_client_ws};
use crate::grpc::db_models::proto_msg_type_to_event_name;

use warp::http::{HeaderMap, HeaderValue};

use lib_clapshot_grpc::proto::client::client_to_server_cmd::{AddComment, DelComment, DelMediaFile, EditComment, ListMyMessages, OpenNavigationPage, OpenMediaFile, RenameMediaFile};
use std::convert::TryFrom;

// ---------------------------------------------------------------------------------------------

#[traced_test]
async fn test_echo()
{
    api_test! {[ws, _ts]
        write(&mut ws, r#"{"cmd":"echo","data":"hello"}"#).await;
        assert_eq!(expect_msg(&mut ws).await, "Echo: hello");
    }
}

#[tokio::test]
#[traced_test]
async fn test_api_push_msg()
{
    api_test! {[ws, ts]
        let mut umsg = UserMessage {
            msg: "test_msg".into(),
            user_id: Some("user.num1".into()),
            details: Some("test_details".into()),
            media_file_id: None,
            topic: UserMessageTopic::Ok,
            subtitle_id: None,
            progress: None
        };

        ts.user_msg_tx.send(umsg.clone()).unwrap();
        let proto_msg = expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;
        assert_eq!(proto_msg.details, Some("test_details".into()));

        // Send to another user, user.num1 should not receive it
        umsg.user_id = Some("user.num2".into());
        ts.user_msg_tx.send(umsg).unwrap();
        expect_no_msg(&mut ws).await;
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_navigation_page()
{
    api_test! {[ws, ts]
        send_server_cmd!(ws, OpenNavigationPage, OpenNavigationPage{..Default::default()});
        let sp = expect_client_cmd!(&mut ws, ShowPage);
        assert_eq!(sp.page_items.len(), 2);

        // Break the database, make sure we get an error
        ts.db.break_db();
        send_server_cmd!(ws, OpenNavigationPage, OpenNavigationPage{..Default::default()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_del_video()
{
    api_test! {[ws, ts]
        let conn = &mut ts.db.conn().unwrap();

        // Delete one successfully
        {
            assert!(models::MediaFile::get(conn, &ts.media_files[0].id).is_ok());

            send_server_cmd!(ws, DelMediaFile, DelMediaFile{media_file_id: ts.media_files[0].id.clone()});
            expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;

            // Make sure the dir is gone
            assert!(matches!(models::MediaFile::get(conn, &ts.media_files[0].id).unwrap_err(), DBError::NotFound()));

            // Make sure it's in trash, and DB row was backed up on disk
            let trash_dir = ts.media_files_dir.join("trash");
            let first_trash_dir = trash_dir.read_dir().unwrap().next().unwrap().unwrap().path();
            let backup_path = first_trash_dir.join("db_backup.json");
            assert!(backup_path.to_string_lossy().contains(&ts.media_files[0].id));
            assert!(backup_path.is_file());
            let backup_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(backup_path).unwrap()).unwrap();
            assert_eq!(backup_json["id"], ts.media_files[0].id);
            assert_eq!(backup_json["orig_filename"], ts.media_files[0].orig_filename.clone().unwrap());
        }

        // Fail to delete a non-existent video
        send_server_cmd!(ws, DelMediaFile, DelMediaFile{media_file_id: "non-existent".into()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Fail to delete someones else's video
        assert!(models::MediaFile::get(conn, &ts.media_files[1].id).is_ok());
        send_server_cmd!(ws, DelMediaFile, DelMediaFile{media_file_id: ts.media_files[1].id.clone()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
        assert!(models::MediaFile::get(conn, &ts.media_files[1].id).is_ok());

        // Break the database
        ts.db.break_db();
        send_server_cmd!(ws, DelMediaFile, DelMediaFile{media_file_id: ts.media_files[2].id.clone()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_open_media_file()
{
    api_test! {[ws, ts]
        for vid in &ts.media_files {
            let v = open_media_file(&mut ws, &vid.id).await.media_file.unwrap();
            assert_eq!(v.id, vid.id);
            assert_eq!(v.user_id.clone(), vid.user_id.clone());
            assert_eq!(v.processing_metadata.unwrap().orig_filename, vid.orig_filename.clone().unwrap());
            assert_eq!(v.title.unwrap(), vid.orig_filename.clone().unwrap());

            // Double slashes (=empty path component) in the path are an error
            let media_url = v.playback_url.unwrap();
            let after_https = media_url.split("://").nth(1).unwrap();
            assert!(!after_https.contains("//"));
        }

        // Break the database
        ts.db.break_db();
        send_server_cmd!(ws, OpenMediaFile, OpenMediaFile{media_file_id: ts.media_files[0].id.clone()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
    }
}

#[tokio::test]
#[traced_test]
async fn test_api_open_bad_media_file()
{
    api_test! {[ws, ts]
        send_server_cmd!(ws, OpenMediaFile, OpenMediaFile{media_file_id: "non-existent".into()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
     }
}

pub async fn expect_user_msg(ws: &mut crate::api_server::test_utils::WsClient, evt_type: proto::user_message::Type ) -> proto::UserMessage
{
    println!(" --expect_user_msg of type {:?} ....", evt_type);
    let cmd = expect_client_cmd!(ws, ShowMessages);
    assert_eq!(cmd.msgs.len(), 1);
    assert_eq!(cmd.msgs[0].r#type, evt_type as i32);
    cmd.msgs[0].clone()
}

#[tokio::test]
#[traced_test]
async fn test_api_rename_media_file()
{
    api_test! {[ws, ts]
        let media_file = &ts.media_files[0];
        open_media_file(&mut ws, &media_file.id).await;
        let conn = &mut ts.db.conn().unwrap();

        // Rename the media file (with leading/trailing whitespace that will be trimmed)
        send_server_cmd!(ws, RenameMediaFile, RenameMediaFile{media_file_id: media_file.id.clone(), new_name: "  New name  ".into()});
        expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;

        // Make sure the media file was renamed in the DB
        let v = models::MediaFile::get(conn, &media_file.id).unwrap();
        assert_eq!(v.title, Some("New name".to_string()));

        // Try to enter an invalid name
        send_server_cmd!(ws, RenameMediaFile, RenameMediaFile{media_file_id: media_file.id.clone(), new_name: " /._  ".into()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Make sure name wasn't changed
        let v = models::MediaFile::get(conn, &media_file.id).unwrap();
        assert_eq!(v.title, Some("New name".to_string()));
    }
}



#[tokio::test]
#[traced_test]
async fn test_api_add_plain_comment()
{
    api_test! {[ws, ts]
        let media = &ts.media_files[0];
        send_server_cmd!(ws, AddComment, AddComment{media_file_id: media.id.clone(), comment: "Test comment".into(), ..Default::default()});

        // No media file opened by the client yet, so no response
        expect_no_msg(&mut ws).await;
        open_media_file(&mut ws, &media.id).await;

        // Add another comment
        let drw_data = "data:image/webp;charset=utf-8;base64,SU1BR0VfREFUQQ==";  // "IMAGE_DATA"

        send_server_cmd!(ws, AddComment, AddComment{media_file_id: media.id.clone(), comment: "Test comment 2".into(), drawing: Some(drw_data.into()), ..Default::default()});
        let c = expect_client_cmd!(&mut ws, AddComments);
        assert_eq!(c.comments.len(), 1);
        assert_eq!(c.comments[0].comment, "Test comment 2");

        // Stored in database, the the image must be path to a file, not the actual image data as data URI
        let cid = i32::from_str(&c.comments[0].id).unwrap();
        assert!(!models::Comment::get(&mut ts.db.conn().unwrap(), &cid).unwrap().drawing.unwrap().contains("data:image"));
        assert!(c.comments[0].clone().drawing.unwrap().starts_with("data:image/webp"));

        // Add a comment to a nonexisting media file
        send_server_cmd!(ws, AddComment, AddComment{media_file_id: "bad_id".into(), comment: "Test comment 3".into(), ..Default::default()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Break the database
        ts.db.break_db();
        send_server_cmd!(ws, AddComment, AddComment{media_file_id: media.id.clone(), comment: "Test comment 5".into(), ..Default::default()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_comment_other_users_video()
{
    api_test! {[ws, ts]
        let other_users_vid = &ts.media_files[1];
        assert_ne!(other_users_vid.user_id, ts.media_files[0].user_id);

        open_media_file(&mut ws, &other_users_vid.id).await;

        // Add comment to someone else's media file
        send_server_cmd!(ws, AddComment, AddComment{media_file_id: other_users_vid.id.clone(), comment: "Comment to other user's video".into(), ..Default::default()});
        let c = expect_client_cmd!(&mut ws, AddComments);
        assert_eq!(c.comments.len(), 1);
        assert_eq!(c.comments[0].comment, "Comment to other user's video");
    }
}

#[tokio::test]
#[traced_test]
async fn test_api_edit_comment()
{
    api_test! {[ws, ts]
        let media = &ts.media_files[0];
        let com = &ts.comments[0];
        open_media_file(&mut ws, &media.id).await;

        // Edit comment
        send_server_cmd!(ws, EditComment, EditComment{comment_id: com.id.to_string(), new_comment: "Edited comment".into(), ..Default::default()});

        let m = expect_client_cmd!(&mut ws, DelComment);
        assert_eq!(m.comment_id, com.id.to_string());

        let m = expect_client_cmd!(&mut ws, AddComments);
        assert_eq!(m.comments.len(), 1);
        assert_eq!(m.comments[0].id, com.id.to_string());
        assert_eq!(m.comments[0].comment, "Edited comment");
        assert_eq!(m.comments[0].media_file_id, media.id);

        assert!(m.comments[0].clone().drawing.unwrap().starts_with("data:image/webp"));
        let drw_data = String::from_utf8( data_url::DataUrl::process(m.comments[0].clone().drawing.unwrap().as_str()).unwrap().decode_to_vec().unwrap().0 ).unwrap();
        assert_eq!(drw_data, "IMAGE_DATA");

        // Edit nonexisting comment
        send_server_cmd!(ws, EditComment, EditComment{comment_id: "1234566999".into(), new_comment: "Edited comment 2".into(), ..Default::default()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Try to edit someone else's comment
        send_server_cmd!(ws, EditComment, EditComment{comment_id: ts.comments[1].id.to_string(), new_comment: "Edited comment 3".into(), ..Default::default()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Break the database
        ts.db.break_db();
        send_server_cmd!(ws, EditComment, EditComment{comment_id: com.id.to_string(), new_comment: "Edited comment 4".into(), ..Default::default()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_del_comment()
{
    // Summary of comment thread used in this test:
    //
    //   media_file[0]:
    //     comment[0] (user 1)
    //       comment[5] (user 2)
    //       comment[6] (user 1)
    //     comment[3] (user 2)

    api_test! {[ws, ts]
        let media = &ts.media_files[0];
        let com = &ts.comments[6];
        open_media_file(&mut ws, &media.id).await;

        // Delete comment[6] (user 1)
        send_server_cmd!(ws, DelComment, DelComment{comment_id: com.id.to_string()});

        let m = expect_client_cmd!(&mut ws, DelComment);
        assert_eq!(m.comment_id, com.id.to_string());

        // Fail to delete nonexisting comment
        send_server_cmd!(ws, DelComment, DelComment{comment_id: "1234566999".into()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Fail to delete user2's comment[3] (user 2)
        send_server_cmd!(ws, DelComment, DelComment{comment_id: ts.comments[3].id.to_string()});
        let m = expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
        assert!(m.message.contains("ermission"));

        // Fail to delete comment[0] that has replies
        send_server_cmd!(ws, DelComment, DelComment{comment_id: ts.comments[0].id.to_string()});
        let m = expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
        assert!(m.details.unwrap().contains("repl"));

        // Delete the last remaining reply comment[5]
        models::Comment::delete(&mut ts.db.conn().unwrap(), &ts.comments[5].id).unwrap(); // Delete from db directly, to avoid user permission check

        // Try again to delete comment id 1 that should now have no replies
        send_server_cmd!(ws, DelComment, DelComment{comment_id: ts.comments[0].id.to_string()});
        expect_client_cmd!(&mut ws, DelComment);
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_list_my_messages()
{
    api_test! {[ws, ts]
        send_server_cmd!(ws, ListMyMessages, ListMyMessages{});

        let m = expect_client_cmd!(&mut ws, ShowMessages);
        assert_eq!(m.msgs.len(), 0);

        let msgs = [
            models::MessageInsert { user_id: "user.num1".into(), message: "message1".into(), event_name: "ok".into(), media_file_id: Some("HASH0".into()), ..Default::default() },
            models::MessageInsert { user_id: "user.num1".into(), message: "message2".into(), event_name: "error".into(), media_file_id: Some("HASH0".into()), details: "STACKTRACE".into(), ..Default::default() },
            models::MessageInsert { user_id: "user.num2".into(), message: "message3".into(), event_name: "ok".into(), ..Default::default() },
        ];
        let msgs = msgs.iter().map(|m| models::Message::insert(&mut ts.db.conn().unwrap(), &m).unwrap()).collect::<Vec<_>>();

        send_server_cmd!(ws, ListMyMessages, ListMyMessages{});
        let sm = expect_client_cmd!(&mut ws, ShowMessages);
        for (i, m) in sm.msgs.iter().enumerate() {
            let mtype = proto::user_message::Type::try_from(m.r#type).unwrap();
            assert_eq!(m.message, msgs[i].message);
            assert_eq!(proto_msg_type_to_event_name(mtype), msgs[i].event_name);
            assert_eq!(m.seen, false);
        }

        // List again, this time messages should be marked "seen"
        send_server_cmd!(ws, ListMyMessages, ListMyMessages{});
        let sm = expect_client_cmd!(&mut ws, ShowMessages);
        assert_eq!(sm.msgs.len(), 2);
        for (i, m) in sm.msgs.iter().enumerate() {
            assert_eq!(m.message, msgs[i].message);
            assert_eq!(m.seen, true);
        }
    }
}


#[tokio::test]
#[traced_test]
async fn test_multipart_upload()
{
    api_test! {[_ws, ts]
        // Upload file
        let file_body = "Testfile 1234";
        let url = format!("http://127.0.0.1:{}/api/upload", ts.port);
        let some_file = multipart::Part::stream(file_body).file_name("testfile.mp4").mime_str("video/mp4").unwrap();
        let form = multipart::Form::new().part("fileupload", some_file);
        let response = Client::new().post(url).multipart(form).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        // Check that file was put in a queue for processing
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(!ts.upload_res_rx.is_empty());
        let up_res = ts.upload_res_rx.recv().unwrap();
        assert_eq!(up_res.file_path.file_name().unwrap(), "testfile.mp4");

        // Verify uploaded file contents
        let mut file = std::fs::File::open(up_res.file_path).unwrap();
        let mut contents = String::new();
        std::io::Read::read_to_string(&mut file, &mut contents).unwrap();
        assert_eq!(contents, file_body);
    }
}


#[test]
fn test_validate_org_http_headers_regex() {
    // Test valid patterns
    assert!(validate_org_http_headers_regex("^X[-_]REMOTE[-_]").is_ok());
    assert!(validate_org_http_headers_regex(".*").is_ok());
    assert!(validate_org_http_headers_regex("test").is_ok());

    // Test invalid patterns
    assert!(validate_org_http_headers_regex("[").is_err());
    assert!(validate_org_http_headers_regex("*").is_err());
}

#[test]
fn test_header_filtering() {
    let regex = validate_org_http_headers_regex("^X[-_]REMOTE[-_]").unwrap();

    // Create test headers
    let mut headers = HeaderMap::new();
    headers.insert("X-Remote-User-Id", HeaderValue::from_static("testuser"));
    headers.insert("X-Remote-User-Name", HeaderValue::from_static("Test User"));
    headers.insert("X-Remote-User-Can-Upload", HeaderValue::from_static("true"));
    headers.insert("X_REMOTE_USER_GROUPS", HeaderValue::from_static("admins,users"));
    headers.insert("Authorization", HeaderValue::from_static("Bearer token123"));
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));

    let (user_id, user_name, is_admin, _cookies, filtered_headers, _remote_error) =
        parse_auth_headers(&headers, "anonymous", &regex);

    // Verify basic auth parsing still works
    assert_eq!(user_id, "testuser");
    assert_eq!(user_name, "Test User");
    assert!(!is_admin); // No X-Remote-User-Is-Admin header, user is not "admin"

    // Verify header filtering (HeaderMap converts names to lowercase)
    assert_eq!(filtered_headers.len(), 4); // 4 X-Remote headers
    assert_eq!(filtered_headers.get("x-remote-user-id"), Some(&"testuser".to_string()));
    assert_eq!(filtered_headers.get("x-remote-user-name"), Some(&"Test User".to_string()));
    assert_eq!(filtered_headers.get("x-remote-user-can-upload"), Some(&"true".to_string()));
    assert_eq!(filtered_headers.get("x_remote_user_groups"), Some(&"admins,users".to_string()));

    // Verify non-matching headers are filtered out (also lowercase)
    assert!(!filtered_headers.contains_key("authorization"));
    assert!(!filtered_headers.contains_key("content-type"));
}

#[test]
fn test_header_filtering_case_insensitive() {
    let regex = validate_org_http_headers_regex("^x-remote-").unwrap();

    let mut headers = HeaderMap::new();
    headers.insert("X-Remote-User-Id", HeaderValue::from_static("testuser"));
    headers.insert("x-remote-groups", HeaderValue::from_static("users"));

    let (_, _, _, _, filtered_headers, _remote_error) =
        parse_auth_headers(&headers, "anonymous", &regex);

    // Both should match due to case insensitive regex (HeaderMap converts to lowercase)
    assert_eq!(filtered_headers.len(), 2);
    assert!(filtered_headers.contains_key("x-remote-user-id"));
    assert!(filtered_headers.contains_key("x-remote-groups"));
}

#[test]
fn test_remote_error_header() {
    let regex = validate_org_http_headers_regex("^X[-_]REMOTE[-_]USER[-_]CAN[-_]UPLOAD$").unwrap();

    // Test with X-Remote-Error header
    let mut headers = HeaderMap::new();
    headers.insert("X-Remote-Error", HeaderValue::from_static("Access denied by IDP"));
    headers.insert("X-Remote-User-Id", HeaderValue::from_static("testuser"));

    let (user_id, _user_name, _is_admin, _cookies, filtered_headers, remote_error) =
        parse_auth_headers(&headers, "anonymous", &regex);

    // Verify error is captured
    assert_eq!(remote_error, Some("Access denied by IDP".to_string()));
    assert_eq!(user_id, "testuser"); // User info should still be parsed

    // Verify error header is NOT passed to organizer (handled by server)
    assert!(!filtered_headers.contains_key("x-remote-error"));

    // Test without error header
    let mut headers_no_error = HeaderMap::new();
    headers_no_error.insert("X-Remote-User-Id", HeaderValue::from_static("testuser"));

    let (_, _, _, _, _, remote_error_none) =
        parse_auth_headers(&headers_no_error, "anonymous", &regex);

    assert_eq!(remote_error_none, None);
}


/// Exercises the notification hook end-to-end through the cheap api_test harness:
/// comment events come from ws commands, media/message events from injecting
/// UserMessages into the same msg_relay the pipeline uses. The harness wires a
/// recording sink (allowlist '*'); we drain `ts.notification_rx` to assert.
#[tokio::test]
#[traced_test]
async fn test_api_notification_hook()
{
    use crate::notification::{NotificationEvent, NotificationKind};

    async fn next_notification(
        rx: &crossbeam_channel::Receiver<NotificationEvent>,
        kind: NotificationKind,
    ) -> serde_json::Value {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let mut seen = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(ev) if ev.kind == kind => return ev.payload,
                Ok(ev) => seen.push(ev.kind.as_str()),
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    assert!(start.elapsed() < Duration::from_secs(5),
                        "Timed out waiting for {} notification; saw {:?}", kind.as_str(), seen);
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(e) => panic!("notification channel closed: {e}"),
            }
        }
    }

    api_test! {[ws, ts]
        let media_id = ts.media_files[0].id.clone();
        let media_owner = ts.media_files[0].user_id.clone();
        open_media_file(&mut ws, &media_id).await;

        // ---- comment_added (plain) ----
        send_server_cmd!(ws, AddComment, AddComment{media_file_id: media_id.clone(), comment: "Hello world".into(), ..Default::default()});
        expect_client_cmd!(&mut ws, AddComments);
        let p = next_notification(&ts.notification_rx, NotificationKind::CommentAdded).await;
        assert_eq!(p["event"], "comment_added");
        assert_eq!(p["actor"]["user_id"], "user.num1");
        assert_eq!(p["actor"]["is_admin"], false);
        assert_eq!(p["comment"]["text"], "Hello world");
        assert_eq!(p["comment"]["author"]["user_id"], "user.num1");
        assert_eq!(p["comment"]["drawing"], serde_json::Value::Null);
        assert_eq!(p["media_file"]["id"], media_id);
        assert_eq!(p["media_file"]["owner_user_id"], media_owner);
        assert!(p["url"].as_str().unwrap().contains(&format!("vid={}", media_id)));
        let cid = p["comment"]["id"].as_str().unwrap().to_string();

        // ---- comment_added with drawing: path must point at an already-written file ----
        let drw = "data:image/webp;charset=utf-8;base64,SU1BR0VfREFUQQ==";
        send_server_cmd!(ws, AddComment, AddComment{media_file_id: media_id.clone(), comment: "With drawing".into(), drawing: Some(drw.into()), ..Default::default()});
        expect_client_cmd!(&mut ws, AddComments);
        let p = next_notification(&ts.notification_rx, NotificationKind::CommentAdded).await;
        let drawing_path = p["comment"]["drawing"].as_str().expect("drawing path present");
        assert!(std::path::Path::new(drawing_path).exists(),
            "drawing file must exist on disk before the notification fires: {drawing_path}");

        // ---- comment_edited carries previous_text ----
        send_server_cmd!(ws, EditComment, EditComment{comment_id: cid.clone(), new_comment: "Edited!".into(), ..Default::default()});
        expect_client_cmd!(&mut ws, DelComment);
        expect_client_cmd!(&mut ws, AddComments);
        let p = next_notification(&ts.notification_rx, NotificationKind::CommentEdited).await;
        assert_eq!(p["comment"]["text"], "Edited!");
        assert_eq!(p["comment"]["previous_text"], "Hello world");
        assert_eq!(p["actor"]["user_id"], "user.num1");

        // ---- comment_deleted ----
        send_server_cmd!(ws, DelComment, DelComment{comment_id: cid.clone()});
        expect_client_cmd!(&mut ws, DelComment);
        let p = next_notification(&ts.notification_rx, NotificationKind::CommentDeleted).await;
        assert_eq!(p["comment"]["id"], cid);
        assert_eq!(p["comment"]["text"], "Edited!");

        // ---- media_file_added (pipeline -> msg_relay) ----
        ts.user_msg_tx.send(UserMessage{
            topic: UserMessageTopic::MediaFileAdded,
            user_id: Some("user.num1".into()),
            media_file_id: Some(media_id.clone()),
            ..Default::default()
        }).unwrap();
        let p = next_notification(&ts.notification_rx, NotificationKind::MediaFileAdded).await;
        assert_eq!(p["event"], "media_file_added");
        assert_eq!(p["media_file"]["id"], media_id);
        assert_eq!(p["media_file"]["owner_user_id"], media_owner);
        assert_eq!(p["actor"], serde_json::Value::Null);

        // ---- media_file_updated carries the change message ----
        ts.user_msg_tx.send(UserMessage{
            topic: UserMessageTopic::MediaFileUpdated,
            user_id: Some("user.num1".into()),
            media_file_id: Some(media_id.clone()),
            msg: "Transcoding done".into(),
            ..Default::default()
        }).unwrap();
        let p = next_notification(&ts.notification_rx, NotificationKind::MediaFileUpdated).await;
        assert_eq!(p["message"], "Transcoding done");
        assert_eq!(p["media_file"]["id"], media_id);

        // ---- message_persisted (pipeline origin) ----
        ts.user_msg_tx.send(UserMessage{
            topic: UserMessageTopic::Ok,
            user_id: Some("user.num1".into()),
            msg: "Job done".into(),
            ..Default::default()
        }).unwrap();
        let p = next_notification(&ts.notification_rx, NotificationKind::MessagePersisted).await;
        assert_eq!(p["origin"], "pipeline");
        assert_eq!(p["message"]["event_name"], "ok");
        assert_eq!(p["message"]["text"], "Job done");
        assert_eq!(p["message"]["recipient_user_id"], "user.num1");
    }
}


/// Full vertical: a real ws comment travels through the sink, the bounded queue,
/// the `run_forever` worker thread, and into an actual (temporary) hook script on
/// disk. Unlike `test_api_notification_hook` (which drains the channel directly),
/// this exercises the worker loop and real process invocation end-to-end.
#[tokio::test]
#[traced_test]
async fn test_notification_hook_runs_real_script()
{
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    // Recording script: append "<event>\t<stdin-json>\n" to a sink file.
    let dir = tempfile::tempdir().unwrap();
    let sink = dir.path().join("hook.log");
    let script = dir.path().join("record.sh");
    std::fs::write(&script, format!(
        "#!/bin/sh\nprintf '%s\\t' \"$CLAPSHOT_NOTIFICATION_EVENT\" >> '{0}'\ncat >> '{0}'\nprintf '\\n' >> '{0}'\n",
        sink.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    api_test! {[ws, ts]
        // Drive the real worker thread off the harness's notification channel.
        let _worker = {
            let rx = ts.notification_rx.clone();
            let script = script.clone();
            std::thread::spawn(move || crate::notification::run_forever(rx, script))
        };

        let media_id = ts.media_files[0].id.clone();
        open_media_file(&mut ws, &media_id).await;

        send_server_cmd!(ws, AddComment, AddComment{media_file_id: media_id.clone(), comment: "Ping from a real script".into(), ..Default::default()});
        expect_client_cmd!(&mut ws, AddComments);

        // Poll the on-disk sink the script writes to. The script appends the record
        // in three steps (event name + tab, then the stdin JSON, then a newline), so
        // only treat it as complete once the trailing newline is present -- otherwise
        // we can race in and read "comment_added\t" before `cat` has written the JSON.
        let start = Instant::now();
        let recorded = loop {
            if let Ok(s) = std::fs::read_to_string(&sink) {
                if s.contains("comment_added") && s.ends_with('\n') { break s; }
            }
            assert!(start.elapsed() < Duration::from_secs(5),
                "hook script never ran; sink still empty");
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        assert!(recorded.starts_with("comment_added\t"), "event name via env: {recorded:?}");
        assert!(recorded.contains("\"text\":\"Ping from a real script\""), "JSON payload via stdin: {recorded:?}");

        // Worker exits on its own once the server (and thus the sink tx) is dropped.
    }
}
