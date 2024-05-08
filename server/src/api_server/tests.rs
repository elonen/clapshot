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
use crate::api_server::{UserMessage, UserMessageTopic, run_api_server_async};
use crate::api_server::server_state::ServerState;
use crate::database::models::{self};
use crate::database::tests::make_test_db;

use crate::api_server::test_utils::{ApiTestState, expect_msg, expect_no_msg, write, open_video, connect_client_ws};
use crate::grpc::db_models::proto_msg_type_to_event_name;

use lib_clapshot_grpc::proto::client::client_to_server_cmd::{AddComment, DelComment, DelVideo, EditComment, ListMyMessages, ListMyVideos, OpenVideo, RenameVideo};
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
            video_id: None, topic: UserMessageTopic::Ok,
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
async fn test_api_list_user_videos()
{
    api_test! {[ws, ts]
        send_server_cmd!(ws, ListMyVideos, ListMyVideos{});
        let sp = expect_client_cmd!(&mut ws, ShowPage);
        assert_eq!(sp.page_items.len(), 2);

        // Break the database, make sure we get an error
        ts.db.break_db();
        send_server_cmd!(ws, ListMyVideos, ListMyVideos{});
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
            assert!(models::Video::get(conn, &ts.videos[0].id).is_ok());

            send_server_cmd!(ws, DelVideo, DelVideo{video_id: ts.videos[0].id.clone()});
            expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;

            // Make sure the dir is gone
            assert!(matches!(models::Video::get(conn, &ts.videos[0].id).unwrap_err(), DBError::NotFound()));

            // Make sure it's in trash, and DB row was backed up on disk
            let trash_dir = ts.videos_dir.join("trash");
            let first_trash_dir = trash_dir.read_dir().unwrap().next().unwrap().unwrap().path();
            let backup_path = first_trash_dir.join("db_backup.json");
            assert!(backup_path.to_string_lossy().contains(&ts.videos[0].id));
            assert!(backup_path.is_file());
            let backup_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(backup_path).unwrap()).unwrap();
            assert_eq!(backup_json["id"], ts.videos[0].id);
            assert_eq!(backup_json["orig_filename"], ts.videos[0].orig_filename.clone().unwrap());
        }

        // Fail to delete a non-existent video
        send_server_cmd!(ws, DelVideo, DelVideo{video_id: "non-existent".into()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Fail to delete someones else's video
        assert!(models::Video::get(conn, &ts.videos[1].id).is_ok());
        send_server_cmd!(ws, DelVideo, DelVideo{video_id: ts.videos[1].id.clone()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
        assert!(models::Video::get(conn, &ts.videos[1].id).is_ok());

        // Break the database
        ts.db.break_db();
        send_server_cmd!(ws, DelVideo, DelVideo{video_id: ts.videos[2].id.clone()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_open_video()
{
    api_test! {[ws, ts]
        for vid in &ts.videos {
            let v = open_video(&mut ws, &vid.id).await.video.unwrap();
            assert_eq!(v.id, vid.id);
            assert_eq!(v.added_by.clone().unwrap().id, vid.user_id.clone().unwrap());
            assert_eq!(v.added_by.clone().unwrap().name.unwrap(), vid.user_name.clone().unwrap());
            assert_eq!(v.processing_metadata.unwrap().orig_filename, vid.orig_filename.clone().unwrap());
            assert_eq!(v.title.unwrap(), vid.orig_filename.clone().unwrap());

            // Double slashes (=empty path component) in the path are an error
            let video_url = v.playback_url.unwrap();
            let after_https = video_url.split("://").nth(1).unwrap();
            assert!(!after_https.contains("//"));
        }

        // Break the database
        ts.db.break_db();
        send_server_cmd!(ws, OpenVideo, OpenVideo{video_id: ts.videos[0].id.clone()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
    }
}

#[tokio::test]
#[traced_test]
async fn test_api_open_bad_video()
{
    api_test! {[ws, ts]
        send_server_cmd!(ws, OpenVideo, OpenVideo{video_id: "non-existent".into()});
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
async fn test_api_rename_video()
{
    api_test! {[ws, ts]
        let video = &ts.videos[0];
        open_video(&mut ws, &video.id).await;
        let conn = &mut ts.db.conn().unwrap();

        // Rename the video (with leading/trailing whitespace that will be trimmed)
        send_server_cmd!(ws, RenameVideo, RenameVideo{video_id: video.id.clone(), new_name: "  New name  ".into()});
        expect_user_msg(&mut ws, proto::user_message::Type::Ok).await;

        // Make sure the video was renamed in the DB
        let v = models::Video::get(conn, &video.id).unwrap();
        assert_eq!(v.title, Some("New name".to_string()));

        // Try to enter an invalid name
        send_server_cmd!(ws, RenameVideo, RenameVideo{video_id: video.id.clone(), new_name: " /._  ".into()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Make sure name wasn't changed
        let v = models::Video::get(conn, &video.id).unwrap();
        assert_eq!(v.title, Some("New name".to_string()));
    }
}



#[tokio::test]
#[traced_test]
async fn test_api_add_plain_comment()
{
    api_test! {[ws, ts]
        let video = &ts.videos[0];
        send_server_cmd!(ws, AddComment, AddComment{video_id: video.id.clone(), comment: "Test comment".into(), ..Default::default()});

        // No video opened by the client yet, so no response
        expect_no_msg(&mut ws).await;
        open_video(&mut ws, &video.id).await;

        // Add another comment
        let drw_data = "data:image/webp;charset=utf-8;base64,SU1BR0VfREFUQQ==";  // "IMAGE_DATA"

        send_server_cmd!(ws, AddComment, AddComment{video_id: video.id.clone(), comment: "Test comment 2".into(), drawing: Some(drw_data.into()), ..Default::default()});

        let c = expect_client_cmd!(&mut ws, AddComments);
        assert_eq!(c.comments.len(), 1);
        assert_eq!(c.comments[0].comment, "Test comment 2");

        // Stored in database, the the image must be path to a file, not the actual image data as data URI
        let cid = i32::from_str(&c.comments[0].id).unwrap();
        assert!(!models::Comment::get(&mut ts.db.conn().unwrap(), &cid).unwrap().drawing.unwrap().contains("data:image"));
        assert!(c.comments[0].clone().drawing.unwrap().starts_with("data:image/webp"));

        // Add a comment to a nonexisting video
        send_server_cmd!(ws, AddComment, AddComment{video_id: "bad_id".into(), comment: "Test comment 3".into(), ..Default::default()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;

        // Break the database
        ts.db.break_db();
        send_server_cmd!(ws, AddComment, AddComment{video_id: video.id.clone(), comment: "Test comment 4".into(), ..Default::default()});
        expect_user_msg(&mut ws, proto::user_message::Type::Error).await;
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_edit_comment()
{
    api_test! {[ws, ts]
        let video = &ts.videos[0];
        let com = &ts.comments[0];
        open_video(&mut ws, &video.id).await;

        // Edit comment
        send_server_cmd!(ws, EditComment, EditComment{comment_id: com.id.to_string(), new_comment: "Edited comment".into(), ..Default::default()});

        let m = expect_client_cmd!(&mut ws, DelComment);
        assert_eq!(m.comment_id, com.id.to_string());

        let m = expect_client_cmd!(&mut ws, AddComments);
        assert_eq!(m.comments.len(), 1);
        assert_eq!(m.comments[0].id, com.id.to_string());
        assert_eq!(m.comments[0].comment, "Edited comment");
        assert_eq!(m.comments[0].video_id, video.id);

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
    //   video[0]:
    //     comment[0] (user 1)
    //       comment[5] (user 2)
    //       comment[6] (user 1)
    //     comment[3] (user 2)

    api_test! {[ws, ts]
        let video = &ts.videos[0];
        let com = &ts.comments[6];
        open_video(&mut ws, &video.id).await;

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
            models::MessageInsert { user_id: "user.num1".into(), message: "message1".into(), event_name: "ok".into(), video_id: Some("HASH0".into()), ..Default::default() },
            models::MessageInsert { user_id: "user.num1".into(), message: "message2".into(), event_name: "error".into(), video_id: Some("HASH0".into()), details: "STACKTRACE".into(), ..Default::default() },
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
