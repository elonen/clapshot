#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing_test::traced_test;
use std::sync::atomic::Ordering::Relaxed;

use reqwest::{multipart, Client};

use crate::database::error::DBError;
use crate::api_server::{UserMessage, UserMessageTopic, run_api_server_async};
use crate::api_server::server_state::ServerState;
use crate::database::models;
use crate::database::tests::make_test_db;

use crate::api_server::test_utils::{ApiTestState, expect_msg, expect_cmd_data, expect_no_msg, write, open_video, connect_client_ws};

// ---------------------------------------------------------------------------------------------

#[tokio::test]
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
            video_hash: None, topic: UserMessageTopic::Ok(), };

        ts.user_msg_tx.send(umsg.clone()).unwrap();
        let (cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(cmd, "message");
        assert_eq!(data["event_name"], "ok");
        assert_eq!(data["details"], "test_details");

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
        write(&mut ws, r#"{"cmd":"list_my_videos","data":{}}"#).await;
        let (cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(cmd, "user_videos");
        assert_eq!(data["user_id"], "user.num1");
        assert_eq!(data["username"], "User Num1");
        assert_eq!(data["videos"].as_array().unwrap().len(), 3);

        // Break the database, make sure we get an error
        ts.db.break_db();
        write(&mut ws, r#"{"cmd":"list_my_videos","data":{}}"#).await;
        let (cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(cmd, "message");
        assert_eq!(data["event_name"], "error");
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_del_video()
{
    api_test! {[ws, ts]

        // Delete one successfully
        {
            assert!(ts.db.get_video(&ts.videos[0].video_hash).is_ok());
            write(&mut ws, &format!(r#"{{"cmd":"del_video","data":{{"video_hash":"{}"}}}}"#, ts.videos[0].video_hash)).await;
            let (_cmd, data) = expect_cmd_data(&mut ws).await;
            assert_eq!(data["event_name"], "ok");
            assert!(!data["details"].as_str().unwrap().contains("WARNING"));

            // Make sure the dir is gone
            assert!(matches!(ts.db.get_video(&ts.videos[0].video_hash).unwrap_err(), DBError::NotFound()));

            // Make sure it's in trash, and DB row was backed up on disk
            let trash_dir = ts.videos_dir.join("trash");
            let first_trash_dir = trash_dir.read_dir().unwrap().next().unwrap().unwrap().path();
            let backup_path = first_trash_dir.join("db_backup.json");
            assert!(backup_path.to_string_lossy().contains(&ts.videos[0].video_hash));
            assert!(backup_path.is_file());
            let backup_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(backup_path).unwrap()).unwrap();
            assert_eq!(backup_json["video_hash"], ts.videos[0].video_hash);
            assert_eq!(backup_json["orig_filename"], ts.videos[0].orig_filename.clone().unwrap());
        }

        // Fail to delete a non-existent video
        write(&mut ws, r#"{"cmd":"del_video","data":{"video_hash":"non-existent"}}"#).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");

        // Fail to delete someones else's video
        assert!(ts.db.get_video(&ts.videos[1].video_hash).is_ok());
        write(&mut ws, &format!(r#"{{"cmd":"del_video","data":{{"video_hash":"{}"}}}}"#, ts.videos[1].video_hash)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");
        assert!(ts.db.get_video(&ts.videos[1].video_hash).is_ok());

        // Break the database
        ts.db.break_db();
        write(&mut ws, &format!(r#"{{"cmd":"del_video","data":{{"video_hash":"{}"}}}}"#, ts.videos[2].video_hash)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_open_video()
{
    api_test! {[ws, ts] 
        for vid in &ts.videos {
            let (_cmd, data) = open_video(&mut ws, &vid.video_hash).await;
            assert_eq!(data["video_hash"], vid.video_hash);
            assert_eq!(data["added_by_userid"], vid.added_by_userid.clone().unwrap());
            assert_eq!(data["added_by_username"], vid.added_by_username.clone().unwrap());
            assert_eq!(data["orig_filename"], vid.orig_filename.clone().unwrap());
            assert_eq!(data["title"], vid.orig_filename.clone().unwrap());

            // Double slashes in the path are an error (empty path component)
            let video_url = data.get("video_url").unwrap().as_str().unwrap();
            let after_https = video_url.split("://").nth(1).unwrap();
            assert!(!after_https.contains("//"));
        }

        // Break the database
        ts.db.break_db();
        write(&mut ws, &format!(r#"{{"cmd":"open_video","data":{{"video_hash":"{}"}}}}"#, ts.videos[0].video_hash)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");
    }
}

#[tokio::test]
#[traced_test]
async fn test_api_open_bad_video()
{
    api_test! {[ws, ts] 
        write(&mut ws, r#"{"cmd":"open_video","data":{"video_hash":"non-existent"}}"#).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");
    }
}

#[tokio::test]
#[traced_test]
async fn test_api_rename_video()
{
    api_test! {[ws, ts] 
        let vid = &ts.videos[0];
        let (_cmd, _data) = open_video(&mut ws, &vid.video_hash).await;

        // Rename the video (with leading/trailing whitespace that will be trimmed)
        write(&mut ws, &format!(r#"{{"cmd":"rename_video","data":{{"video_hash":"{}","new_name":"  New name  "}}}}"#, vid.video_hash)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "ok");

        // Make sure the video was renamed in the DB
        let v = ts.db.get_video(&vid.video_hash).unwrap();
        assert_eq!(v.title, Some("New name".to_string()));

        // Try to enter an invalid name
        write(&mut ws, &format!(r#"{{"cmd":"rename_video","data":{{"video_hash":"{}","new_name":" /._  "}}}}"#, vid.video_hash)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");

        // Make sure name wasn't changed
        let v = ts.db.get_video(&vid.video_hash).unwrap();
        assert_eq!(v.title, Some("New name".to_string()));
    }
}



#[tokio::test]
#[traced_test]
async fn test_api_add_plain_comment()
{
    api_test! {[ws, ts] 
        let vid = &ts.videos[0];
        write(&mut ws, &format!(r#"{{"cmd":"add_comment","data":{{"video_hash":"{}","comment":"Test comment"}}}}"#, vid.video_hash)).await;

        // No video opened by the client yet, so no response
        expect_no_msg(&mut ws).await;
        let (_cmd, _data) = open_video(&mut ws, &vid.video_hash).await;

        // Add another comment
        let drw_data = "data:image/webp;charset=utf-8;base64,SU1BR0VfREFUQQ==";  // "IMAGE_DATA"

        write(&mut ws, &format!(r#"{{"cmd":"add_comment","data":{{"video_hash":"{}","comment":"Test comment 2","drawing":"{}"}}}}"#, vid.video_hash, drw_data)).await;

        let (cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(cmd, "new_comment");
        assert_eq!(data["comment"], "Test comment 2");

        // Stored in database, the the image must be path to a file, not the actual image data as data URI
        assert!(!ts.db.get_comment(data["comment_id"].as_i64().unwrap() as i32).unwrap().drawing.unwrap().contains("data:image"));
        assert!(data["drawing"].as_str().unwrap().starts_with("data:image/webp"));

        // Add a comment to a nonexisting video
        write(&mut ws, r#"{"cmd":"add_comment","data":{"video_hash":"bad_hash","comment":"Test comment 3"}}"#).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");

        // Break the database
        ts.db.break_db();
        write(&mut ws, &format!(r#"{{"cmd":"add_comment","data":{{"video_hash":"{}","comment":"Test comment 4"}}}}"#, vid.video_hash)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_edit_comment()
{
    api_test! {[ws, ts] 
        let vid = &ts.videos[0];
        let com = &ts.comments[0];
        open_video(&mut ws, &vid.video_hash).await;

        // Edit comment
        write(&mut ws, &format!(r#"{{"cmd":"edit_comment","data":{{"comment_id":{},"comment":"Edited comment"}}}}"#, com.id)).await;
        let (cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(cmd, "del_comment");
        assert_eq!(data["comment_id"], com.id);
        let (cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(cmd, "new_comment");
        assert_eq!(data["comment_id"], com.id);
        assert_eq!(data["comment"], "Edited comment");
        assert_eq!(data["video_hash"], vid.video_hash);

        assert!(data["drawing"].as_str().unwrap().starts_with("data:image/webp"));
        let drw_data = String::from_utf8( data_url::DataUrl::process(data["drawing"].as_str().unwrap()).unwrap().decode_to_vec().unwrap().0 ).unwrap();
        assert_eq!(drw_data, "IMAGE_DATA");

        // Edit nonexisting comment
        write(&mut ws, r#"{"cmd":"edit_comment","data":{"comment_id":1234566999,"comment":"Edited comment 2"}}"#).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");

        // Try to edit someone else's comment
        write(&mut ws, &format!(r#"{{"cmd":"edit_comment","data":{{"comment_id":{},"comment":"Edited comment 3"}}}}"#, ts.comments[1].id)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");

        // Break the database
        ts.db.break_db();
        write(&mut ws, &format!(r#"{{"cmd":"edit_comment","data":{{"comment_id":{},"comment":"Edited comment 4"}}}}"#, com.id)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");
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
        let vid = &ts.videos[0];
        let com = &ts.comments[6];
        open_video(&mut ws, &vid.video_hash).await;

        // Delete comment[6] (user 1)
        write(&mut ws, &format!(r#"{{"cmd":"del_comment","data":{{"comment_id":{}}}}}"#, com.id)).await;
        let (cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(cmd, "del_comment");
        assert_eq!(data["comment_id"], com.id);

        // Fail to delete nonexisting comment
        write(&mut ws, r#"{"cmd":"del_comment","data":{"comment_id":1234566999}}"#).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");

        // Fail to delete user2's comment[3] (user 2)
        write(&mut ws, &format!(r#"{{"cmd":"del_comment","data":{{"comment_id":{}}}}}"#, ts.comments[3].id)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");
        assert!(data["details"].as_str().unwrap().contains("your"));

        // Fail to delete comment[0] that has replies
        write(&mut ws, &format!(r#"{{"cmd":"del_comment","data":{{"comment_id":{}}}}}"#, ts.comments[0].id)).await;
        let (_cmd, data) = expect_cmd_data(&mut ws).await;
        assert_eq!(data["event_name"], "error");
        assert!(data["details"].as_str().unwrap().contains("repl"));

        // Delete the last remaining reply comment[5]
        ts.db.del_comment(ts.comments[5].id).unwrap();  // Delete from db directly, to avoid user permission check

        // Try again to delete comment id 1 that should now have no replies
        write(&mut ws, &format!(r#"{{"cmd":"del_comment","data":{{"comment_id":{}}}}}"#, ts.comments[0].id)).await;
        let (cmd, _) = expect_cmd_data(&mut ws).await;
        assert_eq!(cmd, "del_comment");
    }
}


#[tokio::test]
#[traced_test]
async fn test_api_list_my_messages()
{
    api_test! {[ws, ts] 
        write(&mut ws, r#"{"cmd":"list_my_messages","data":{}}"#).await;
        expect_no_msg(&mut ws).await;

        let msgs = [
            models::MessageInsert { user_id: "user.num1".into(), message: "message1".into(), event_name: "info".into(), ref_video_hash: Some("HASH0".into()), ..Default::default() },
            models::MessageInsert { user_id: "user.num1".into(), message: "message2".into(), event_name: "oops".into(), ref_video_hash: Some("HASH0".into()), details: "STACKTRACE".into(), ..Default::default() },
            models::MessageInsert { user_id: "user.num2".into(), message: "message3".into(), event_name: "info".into(), ..Default::default() },
        ];
        let msgs = msgs.iter().map(|m| ts.db.add_message(m).unwrap()).collect::<Vec<_>>();

        write(&mut ws, r#"{"cmd":"list_my_messages","data":{}}"#).await;
        for m in msgs.iter().filter(|m| m.user_id == "user.num1") {
            let (cmd, data) = expect_cmd_data(&mut ws).await;
            assert_eq!(cmd, "message");
            assert_eq!(data["event_name"], m.event_name);
            assert_eq!(data["seen"], false);
        }
        expect_no_msg(&mut ws).await;  // No more messages

        // List again, this time messages should be marked "seen"
        write(&mut ws, r#"{"cmd":"list_my_messages","data":{}}"#).await;
        for m in msgs.iter().filter(|m| m.user_id == "user.num1") {
            let (cmd, data) = expect_cmd_data(&mut ws).await;
            assert_eq!(cmd, "message");
            assert_eq!(data["event_name"], m.event_name);
            assert_eq!(data["seen"], true);
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
