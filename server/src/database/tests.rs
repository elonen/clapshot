use tracing_test::traced_test;
use crate::{database::*, grpc::db_message_to_proto3};



/// Create a temporary database and populate it for testing.
///
/// Contents are roughly as follows:
/// ```text
/// <Video(id=HASH0 orig_filename=test0.mp4 added_by_userid=user.num1 ...)>
/// <Video(id=1111 orig_filename=test1.mp4 added_by_userid=user.num2 ...)>
/// <Video(id=22222 orig_filename=test2.mp4 added_by_userid=user.num1 ...)>
/// <Video(id=HASH3 orig_filename=test3.mp4 added_by_userid=user.num2 ...)>
/// <Video(id=HASH4 orig_filename=test4.mp4 added_by_userid=user.num1 ...)>
/// <Comment(id='1' video=HASH0 parent=None user_id='user.num1' comment='Comment 0' has-drawing=True ...)>
/// <Comment(id='2' video=1111 parent=None user_id='user.num2' comment='Comment 1' has-drawing=True ...)>
/// <Comment(id='3' video=22222 parent=None user_id='user.num1' comment='Comment 2' has-drawing=True ...)>
/// <Comment(id='4' video=HASH0 parent=None user_id='user.num2' comment='Comment 3' has-drawing=True ...)>
/// <Comment(id='5' video=1111 parent=None user_id='user.num1' comment='Comment 4' has-drawing=True ...)>
/// <Comment(id='6' video=HASH0 parent=1 user_id='user.num2' comment='Comment 5' has-drawing=True ...)>
/// <Comment(id='7' video=HASH0 parent=1 user_id='user.num1' comment='Comment 6' has-drawing=True ...)>
/// ```
pub fn make_test_db() -> (std::sync::Arc<DB>, assert_fs::TempDir, Vec<models::Video>, Vec<models::Comment>)
{
    let data_dir = assert_fs::TempDir::new().unwrap();

    let db = DB::connect_db_url(":memory:").unwrap();
    db.run_migrations().unwrap();

    // Make some videos
    let hashes = vec!["HASH0", "11111", "22222", "HASH3", "HASH4"];
    let mkvid = |i: usize| {
        let v = models::VideoInsert {
            id: hashes[i].to_string(),
            added_by_userid: Some(format!("user.num{}", 1 + i % 2)),
            added_by_username: Some(format!("User Number{}", 1 + i % 2)),
            orig_filename: Some(format!("test{}.mp4", i)),
            title: Some(format!("test{}.mp4", i)),
            recompression_done: None,
            thumb_sheet_cols: None,
            thumb_sheet_rows: None,
            total_frames: Some((i * 1000) as i32),
            duration: Some((i * 100) as f32),
            fps: Some(format!("{}", i * i)),
            raw_metadata_all: Some(format!("{{all: {{video: {}}}}}", i)),
        };
        db.add_video(&v).unwrap();
        db.get_video(&v.id).unwrap()
    };
    let videos = (0..5).map(mkvid).collect::<Vec<_>>();

    // Make some comments
    let mkcom = |i: usize, vid: &str, parent_id: Option<i32>| {
        let c = models::CommentInsert {
            video_id: vid.to_string(),
            parent_id,
            timecode: None,
            user_id: format!("user.num{}", 1 + i % 2),
            username: format!("User Number{}", 1 + i % 2),
            comment: format!("Comment {}", i),
            drawing: Some(format!("drawing_{}.webp", i)),
        };
        let id = db.add_comment(&c).unwrap();
        let c = db.get_comment(&id).unwrap();
        let dp = data_dir.join("videos").join(vid).join("drawings");
        std::fs::create_dir_all(&dp).unwrap();
        std::fs::write(dp.join(&c.drawing.clone().unwrap()), "IMAGE_DATA").unwrap();
        c
    };
    let mut comments = (0..5)
        .map(|i| mkcom(i, &videos[i % 3].id, None))
        .collect::<Vec<_>>();
    let more_comments = (5..5 + 2)
        .map(|i| mkcom(i, &comments[0].video_id, Some(comments[0].id)))
        .collect::<Vec<_>>();
    comments.extend(more_comments);

    // Add another comment with empty drawing (caused an error at one point)
    let c = models::CommentInsert {
        video_id: videos[0].id.clone(),
        parent_id: None,
        timecode: None,
        user_id: "user.num1".to_string(),
        username: "User Number1".to_string(),
        comment: "Comment_with_empty_drawing".to_string(),
        drawing: Some("".into()),
    };
    db.add_comment(&c).unwrap();

    (std::sync::Arc::new(db), data_dir, videos, comments)
}


#[test]
#[traced_test]
fn test_fixture_state() -> anyhow::Result<()>
{
    let (db, _data_dir, videos, comments) = make_test_db();

    // First 5 comments have no parent, last 2 have parent_id=1
    for i in 0..5 { assert!(comments[i].parent_id.is_none()); }
    for i in 5..5 + 2 { assert_eq!(comments[i].parent_id, Some(comments[0].id)); }

    // Video #0 has 3 comments, video #1 has 2, video #2 has 1
    assert_eq!(comments[0].video_id, comments[3].video_id);
    assert_eq!(comments[0].video_id, comments[5].video_id);
    assert_eq!(comments[0].video_id, comments[6].video_id);
    assert_eq!(comments[0].video_id, videos[0].id);
    assert_eq!(comments[1].video_id, comments[4].video_id);
    assert_eq!(comments[1].video_id, videos[1].id);
    assert_eq!(comments[2].video_id, videos[2].id);

    // Read entries from database and check that they match definitions
    for v in videos.iter() {
        assert_eq!(db.get_video(&v.id)?.id, v.id);
        let comments = db.get_video_comments(&v.id)?;
        assert_eq!(comments.len(), match v.id.as_str() {
            "HASH0" => 5,
            "11111" => 2,
            "22222" => 1,
            "HASH3" => 0,
            "HASH4" => 0,
            _ => panic!("Unexpected video id"),
        });
    }
    for c in comments.iter() {
        assert_eq!(db.get_comment(&c.id.to_string())?.id, c.id);
        assert_eq!(db.get_comment(&c.id.to_string())?.comment, c.comment);
    }

    // Check that we can get videos by user
    assert_eq!(db.get_all_user_videos("user.num1")?.len(), 3);
    assert_eq!(db.get_all_user_videos("user.num2")?.len(), 2);
    Ok(())
}


#[test]
#[traced_test]
fn test_comment_delete() -> anyhow::Result<()> {
    let (db, _data_dir, _vid, com) = make_test_db();

    assert_eq!(db.get_video_comments(&com[1].video_id)?.len(), 2, "Video should have 2 comments before deletion");

    // Delete comment #2 and check that it was deleted, and nothing else
    db.del_comment(&com[1].id.to_string())?;
    for c in com.iter() {
        if c.id == com[1].id {
            assert!(matches!(db.get_comment(&c.id.to_string()).unwrap_err() , DBError::NotFound()), "Comment should be deleted");
        } else {
            assert_eq!(db.get_comment(&c.id.to_string())?.id, c.id, "Deletion removed wrong comment(s)");
        }
    }

    // Check that video still has 1 comment
    assert_eq!(db.get_video_comments(&com[1].video_id)?.len(), 1, "Video should have 1 comment left");

    // Delete last, add a new one and check for ID reuse
    db.del_comment(&com[6].id.to_string())?;
    let c = models::CommentInsert {
        video_id: com[1].video_id.clone(),
        parent_id: None,
        user_id: com[1].user_id.clone(),
        username: "name".to_string(),
        comment: "re-add".to_string(),
        timecode: None,
        drawing: None,
    };
    let new_id = db.add_comment(&c)?;
    assert_ne!(new_id, com[6].id.to_string(), "Comment ID was re-used after deletion. This would mix up comment threads in the UI.");
    Ok(())
}

#[test]
#[traced_test]
fn test_rename_video() -> anyhow::Result<()> {
    let (db, _data_dir, _vid, _com) = make_test_db();

    // Rename video #1
    let new_name = "New name";
    db.rename_video(&"11111".to_string(), new_name)?;

    // Check that video #1 has new name
    let v = db.get_video(&"11111".to_string())?;
    assert_eq!(v.title, Some(new_name.into()));

    // Check that video #2 still has old name
    let v = db.get_video(&"22222".to_string())?;
    assert_ne!(v.title, Some(new_name.into()));

    Ok(())
}


#[test]
#[traced_test]
fn test_user_messages() -> anyhow::Result<()> {
    let (db, _data_dir, _vid, _com) = make_test_db();

    // Add a message to user #1
    let msgs = [
        models::MessageInsert {
            user_id: "user.num1".into(),
            message: "message1".into(),
            event_name: "info".into(),
            ref_video_id: Some("HASH0".into()),
            ref_comment_id: None,
            details: "".into(),
            seen: false,
        },
        models::MessageInsert {
            user_id: "user.num1".into(),
            message: "message2".into(),
            event_name: "oops".into(),
            ref_video_id: Some("HASH0".into()),
            ref_comment_id: None,
            details: "STACKTRACE".into(),
            seen: false,
        },
        models::MessageInsert {
            user_id: "user.num2".into(),
            message: "message3".into(),
            event_name: "info".into(),
            ref_video_id: None,
            ref_comment_id: None,
            details: "".into(),
            seen: false,
        },
    ];

    let mut new_msgs = vec![];
    for i in 0..msgs.len() {
        let new_msg = db.add_message(&msgs[i])?;
        assert_eq!(new_msg.user_id, msgs[i].user_id);
        assert_eq!(new_msg.message, msgs[i].message);

        let a = serde_json::to_value(db_message_to_proto3(&db.get_message(new_msg.id)?))?;
        let b = serde_json::to_value(db_message_to_proto3(&new_msg))?;
        assert_eq!(a,b);

        assert!(!db.get_message(new_msg.id)?.seen);
        new_msgs.push(new_msg);
    }

    // Correctly count messages
    assert_eq!(db.get_user_messages("user.num1")?.len(), 2);
    assert_eq!(db.get_user_messages("user.num2")?.len(), 1);

    // Mark message #2 as seen
    db.set_message_seen(new_msgs[1].id, true)?;
    assert!(db.get_message(new_msgs[1].id)?.seen);

    // Delete & recount
    db.del_message(new_msgs[2].id)?;
    db.del_message(new_msgs[0].id)?;
    assert_eq!(db.get_user_messages("user.num1")?.len(), 1);
    assert_eq!(db.get_user_messages("user.num2")?.len(), 0);

    Ok(())
}
