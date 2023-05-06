use tracing_test::traced_test;
use crate::{database::*};

use models::{Video, VideoInsert, Message, MessageInsert, Comment, CommentInsert, PropNode, PropNodeInsert, PropEdge, PropEdgeInsert};

macro_rules! test_insert_node {
    ($db:expr, $type:expr, $body:expr) => {
        PropNode::insert(&$db, &PropNodeInsert {
            node_type: $type.to_string(),
            body: $body.map(|s: &str| s.to_string()),
        }).expect("Failed to insert test node")
    };
}

macro_rules! test_insert_edge {
    ($db:ident, $from_type:ident, $from_expr:expr, $to_type:ident, $to_expr:expr, $edge_type:expr, $sort_order:expr, $body:expr) => {
        PropEdge::insert(&$db, &PropEdgeInsert {
            $from_type: Some($from_expr.clone()),
            $to_type: Some($to_expr.clone()),
            edge_type: $edge_type.to_string(),
            sort_order: Some($sort_order as f32),
            body: $body.map(|s: &str| s.to_string()),
            ..Default::default()
        }).expect("Failed to insert test edge")
    };
}

fn _dump_db(db: &DB) {
    println!("================ dump_db ================");

    let videos = Video::get_all(db, DBPaging::default()).unwrap();
    println!("----- Videos -----");
    for v in videos { println!("----\n{:#?}", v);}

    let comments = Comment::get_all(db, DBPaging::default()).unwrap();
    println!("----- Comments -----");
    for c in comments { println!("----\n{:#?}", c);}

    let messages = Message::get_all(db, DBPaging::default()).unwrap();
    println!("----- Messages -----");
    for m in messages { println!("----\n{:#?}", m);}

    let nodes = PropNode::get_all(db, DBPaging::default()).unwrap();
    println!("----- Nodes -----");
    for n in nodes { println!("----\n{:#?}", n);}

    let edges = PropEdge::get_all(db, DBPaging::default()).unwrap();
    println!("----- Edges -----");
    for e in edges { println!("----\n{:#?}", e);}

    println!("=========================================");
}

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
pub fn make_test_db() -> (std::sync::Arc<DB>, assert_fs::TempDir, Vec<Video>, Vec<Comment>, Vec<PropNode>, Vec<PropEdge>)
{
println!("--- make_test_db");

    let data_dir = assert_fs::TempDir::new().unwrap();

    let db = DB::connect_db_url(":memory:").unwrap();
    db.run_migrations().unwrap();

    // Make some videos
    let hashes = vec!["HASH0", "11111", "22222", "HASH3", "HASH4"];
    let mkvid = |i: usize| {
        let v = VideoInsert {
            id: hashes[i].to_string(),
            user_id: Some(format!("user.num{}", 1 + i % 2)),
            user_name: Some(format!("User Number{}", 1 + i % 2)),
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
        Video::insert(&db, &v).unwrap();
        Video::get(&db, &v.id.into()).unwrap()
    };
    let videos = (0..5).map(mkvid).collect::<Vec<_>>();

    // Make some comments
    let mkcom = |i: usize, vid: &str, parent_id: Option<i32>| {
        let c = CommentInsert {
            video_id: vid.to_string(),
            parent_id,
            timecode: None,
            user_id: format!("user.num{}", 1 + i % 2),
            user_name: format!("User Number{}", 1 + i % 2),
            comment: format!("Comment {}", i),
            drawing: Some(format!("drawing_{}.webp", i)),
        };
        let c = Comment::insert(&db, &c).unwrap();
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

    // Add another comment (#8) with empty drawing (caused an error at one point)
    let c = CommentInsert {
        video_id: videos[0].id.clone(),
        parent_id: None,
        timecode: None,
        user_id: "user.num1".to_string(),
        user_name: "User Number1".to_string(),
        comment: "Comment_with_empty_drawing".to_string(),
        drawing: Some("".into()),
    };
    let cmt = models::Comment::insert(&db, &c).unwrap();
    comments.push(cmt);

    // Make a test props graph

    let nodes = vec![
        test_insert_node!(&db, "node_type_a", Some("node_body0")),
        test_insert_node!(&db, "node_type_b", Some("node_body1")),
        test_insert_node!(&db, "node_type_c", Some("node_body2")),
        test_insert_node!(&db, "node_type_c", None),
        test_insert_node!(&db, "node_type_d", Some("node_body4")),
    ];

    let edges = vec![
        test_insert_edge!(db, from_node,nodes[0].id, to_node,nodes[1].id, "edge_type_a", 0, Some("edge_body0")),
        test_insert_edge!(db, from_node,nodes[0].id, to_node,nodes[1].id, "edge_type_b", 1, Some("edge_body1")),
        test_insert_edge!(db, from_node,nodes[0].id, to_node,nodes[2].id, "edge_type_c", 2, None),

        test_insert_edge!(db, from_node,nodes[0].id, to_video,videos[0].id, "edge_type_a", 3, Some("edge_body3")),
        test_insert_edge!(db, from_node,nodes[1].id, to_video,videos[0].id, "edge_type_a", 4, None),
        test_insert_edge!(db, from_node,nodes[0].id, to_video,videos[1].id, "edge_type_b", 5, Some("edge_body5")),

        test_insert_edge!(db, from_video,videos[1].id, to_node,nodes[2].id, "edge_type_a", 6, Some("edge_body6")),
        test_insert_edge!(db, from_video,videos[2].id, to_node,nodes[2].id, "edge_type_b", 7, Some("edge_body7")),
        test_insert_edge!(db, from_video,videos[2].id, to_node,nodes[3].id, "edge_type_b", 8, Some("edge_body8")),

        test_insert_edge!(db, from_node,nodes[0].id, to_comment,comments[0].id, "edge_type_c", 9, Some("edge_body9")),
        test_insert_edge!(db, from_node,nodes[1].id, to_comment,comments[0].id, "edge_type_c", 10, None),
        test_insert_edge!(db, from_node,nodes[0].id, to_comment,comments[1].id, "edge_type_b", 11, Some("edge_body11")),

        test_insert_edge!(db, from_comment,comments[0].id, to_node,nodes[2].id, "edge_type_a", 12, Some("edge_body12")),
        test_insert_edge!(db, from_comment,comments[1].id, to_node,nodes[2].id, "edge_type_b", 13, Some("edge_body13")),
        test_insert_edge!(db, from_comment,comments[1].id, to_node,nodes[3].id, "edge_type_b", 14, Some("edge_body14")),

        test_insert_edge!(db, from_comment,comments[0].id, to_video,videos[0].id, "edge_type_a",    15, Some("edge_body15")),
        test_insert_edge!(db, from_video,videos[3].id,     to_video,videos[3].id, "edge_type_loop", 16, None),
    ];

    (std::sync::Arc::new(db), data_dir, videos, comments, nodes, edges)
}


#[test]
#[traced_test]
fn test_pagination() -> anyhow::Result<()> {
    let (db, _data_dir, _videos, comments, _nodes, _edges) = make_test_db();

    // Test pagination of comments
    let mut res = Comment::get_all(&db, DBPaging { page_num: 0, page_size: 3.try_into()? })?;
    println!("---- page 0, 3");
    println!("res: {:#?}", res);

    assert_eq!(res.len(), 3);
    assert_eq!(res[0].id, comments[0].id);
    assert_eq!(res[1].id, comments[1].id);
    assert_eq!(res[2].id, comments[2].id);

    res = Comment::get_all(&db, DBPaging { page_num: 1, page_size: 3.try_into()? })?;
    println!("---- page 1, 3");
    println!("res: {:#?}", res);
    assert_eq!(res.len(), 3);
    assert_eq!(res[0].id, comments[3].id);
    assert_eq!(res[1].id, comments[4].id);
    assert_eq!(res[2].id, comments[5].id);

    res = Comment::get_all(&db, DBPaging { page_num: 2, page_size: 3.try_into()? })?;
    println!("---- page 2, 3");
    println!("res: {:#?}", res);
    assert_eq!(res.len(), 2);
    assert_eq!(res[0].id, comments[6].id);
    assert_eq!(res[1].id, comments[7].id);

    Ok(())
}



#[test]
#[traced_test]
fn test_fail_invalid_edge_inserts() -> anyhow::Result<()> {
    let (db, _data_dir, videos, comments, nodes, _edges) = make_test_db();

    // Insert an edge with no from_*
    let e = PropEdgeInsert {
        to_video: Some(videos[0].id.clone()), ..Default::default()
    };
    assert!(PropEdge::insert(&db, &e).is_err());

    // Insert an edge with no to_*
    let e = PropEdgeInsert {
        from_video: Some(videos[0].id.clone()), ..Default::default()
    };
    assert!(PropEdge::insert(&db, &e).is_err());

    // Insert an edge with multiple from_*
    let e = PropEdgeInsert {
        from_video: Some(videos[0].id.clone()),
        from_comment: Some(comments[0].id.clone()), ..Default::default()
    };
    assert!(PropEdge::insert(&db, &e).is_err());

    // Insert an edge with multiple to_*
    let e = PropEdgeInsert {
        to_node: Some(nodes[0].id.clone()),
        to_comment: Some(comments[0].id.clone()), ..Default::default()
    };
    assert!(PropEdge::insert(&db, &e).is_err());

    Ok(())
}


#[test]
#[traced_test]
fn test_graph_db_pointing_queries() -> anyhow::Result<()> {
    let (db, _data_dir, videos, comments, nodes, edges) = make_test_db();

    // Videos pointing to nodes

    assert_eq!(Video::graph_get_by_parent(&db, GraphObjId::Node(nodes[3].id), None)?.len(), 1);
    assert_eq!(Video::graph_get_by_parent(&db, GraphObjId::Node(nodes[4].id), None)?.len(), 0);

    let res = Video::graph_get_by_parent(&db, GraphObjId::Node(nodes[2].id), None)?;
    assert_eq!(res.len(), 2);
    assert_eq!(res[0].edge.from_video, Some(res[0].obj.id.clone()));

    assert_eq!(res[0].obj.id, videos[1].id);
    assert_eq!(res[0].obj.user_id, videos[1].user_id);
    assert_eq!(res[0].edge.to_node, Some(nodes[2].id));
    assert_eq!(res[0].edge.edge_type, "edge_type_a");
    assert_eq!(res[0].edge.body, Some("edge_body6".to_string()));
    assert_eq!(res[0].edge.sort_order, Some(6.0));

    assert_eq!(res[1].obj.id, videos[2].id);
    assert_eq!(res[1].obj.user_id, videos[2].user_id);
    assert_eq!(res[1].edge.to_node, Some(nodes[2].id));
    assert_eq!(res[1].edge.edge_type, "edge_type_b");
    assert_eq!(res[1].edge.body, Some("edge_body7".to_string()));
    assert_eq!(res[1].edge.sort_order, Some(7.0));

    let res = Video::graph_get_by_parent(&db, GraphObjId::Node(nodes[2].id), Some("edge_type_b"))?;
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].obj.id, videos[2].id);

    // Nodes pointing to videos
    assert_eq!(PropNode::graph_get_by_parent(&db, GraphObjId::Video(&videos[0].id), None)?.len(), 2);
    assert_eq!(PropNode::graph_get_by_parent(&db, GraphObjId::Video(&videos[1].id), None)?.len(), 1);

    let res = PropNode::graph_get_by_parent(&db, GraphObjId::Video(&videos[0].id), None)?;
    assert_eq!(res.len(), 2);
    assert_eq!(res[0].edge.from_node, Some(nodes[0].id));
    assert_eq!(res[1].edge.from_node, Some(nodes[1].id));
    assert_eq!(res[0].edge.to_video, Some(videos[0].id.clone()));
    assert_eq!(res[1].edge.to_video, Some(videos[0].id.clone()));
    assert_eq!(res[0].edge.body, Some("edge_body3".to_string()));
    assert_eq!(res[1].edge.body, None);

    // Nodes pointing to nodes
    let res = PropNode::graph_get_by_parent(&db, GraphObjId::Node(nodes[1].id), None)?;
    assert_eq!(res.len(), 2);

    assert_eq!(res[0].edge.from_node, Some(nodes[0].id));
    assert_eq!(res[0].edge.to_node, Some(nodes[1].id));
    assert_eq!(res[0].edge.edge_type, "edge_type_a");
    assert_eq!(res[0].edge.body, Some("edge_body0".to_string()));

    assert_eq!(res[1].edge.from_node, Some(nodes[0].id));
    assert_eq!(res[1].edge.to_node, Some(nodes[1].id));
    assert_eq!(res[1].edge.edge_type, "edge_type_b");
    assert_eq!(res[1].edge.body, Some("edge_body1".to_string()));

    assert_eq!(PropNode::graph_get_by_parent(&db, GraphObjId::Node(nodes[1].id), Some("edge_type_a"))?.len(), 1);
    assert_eq!(PropNode::graph_get_by_parent(&db, GraphObjId::Node(nodes[1].id), Some("NO_SUCH_TYPE"))?.len(), 0);

    let res = Comment::graph_get_by_child(&db, GraphObjId::Node(nodes[0].id), None)?;
    assert_eq!(res.len(), 2);
    assert_eq!(res[0].obj.id, comments[0].id);
    assert_eq!(res[1].obj.id, comments[1].id);

    let res = PropNode::graph_get_parentless(&db, None)?;
    assert_eq!(res.len(), 3);
    assert_eq!(res[0].id, nodes[2].id);
    assert_eq!(res[1].id, nodes[3].id);
    assert_eq!(res[2].id, nodes[4].id);

    let res = Video::graph_get_parentless(&db, None)?;
    assert_eq!(res.len(), 2);
    assert_eq!(res[0].id, videos[0].id);
    assert_eq!(res[1].id, videos[4].id);

    let res = Video::graph_get_parentless(&db, Some("edge_type_a"))?;
    assert_eq!(res.len(), 4);
    assert!(res.iter().find(|v| v.id == videos[1].id).is_none());

    let res = Video::graph_get_parentless(&db, Some("edge_type_b"))?;
    assert_eq!(res.len(), 4);
    assert!(res.iter().find(|v| v.id == videos[2].id).is_none());


    let res = PropNode::graph_get_childless(&db, None)?;
    assert_eq!(res.len(), 2);
    assert!(res.iter().find(|v| v.id == nodes[0].id).is_some());
    assert!(res.iter().find(|v| v.id == nodes[4].id).is_some());

    let res = Video::graph_get_childless(&db, Some("edge_type_a"))?;
    assert_eq!(res.len(), 4);
    assert!(res.iter().find(|v| v.id == videos[0].id).is_none());

    assert_eq!(PropEdge::get_all(&db, DBPaging::default())?.len(), 17);

    let et_a = PropEdge::get_filtered(&db, None, None, Some("edge_type_a"), &None, DBPaging::default())?;
    assert_eq!(et_a.len(), 6);
    let et_loop = PropEdge::get_filtered(&db, None, None, Some("edge_type_loop"), &None, DBPaging::default())?;
    assert_eq!(et_loop.len(), 1);

    let edge_ids = &[edges[0].id, edges[2].id];
    assert_eq!(PropEdge::get_many(&db, edge_ids)?.len(), 2);
    assert_eq!( PropEdge::get_filtered(&db, None, None, None, &Some(edge_ids.to_vec()), DBPaging::default())?.len(), 2);

    Ok(())
}

#[test]
#[traced_test]
fn test_graph_db_delete_nodes() -> anyhow::Result<()> {
    let (db, _data_dir, videos, _comments, nodes, _edges) = make_test_db();

    assert_eq!(PropNode::get_all(&db, DBPaging::default())?.len(), 5);
    assert_eq!(PropEdge::get_all(&db, DBPaging::default())?.len(), 17);

    PropNode::delete_many(&db, &[nodes[0].id, nodes[1].id])?;
    assert_eq!(PropNode::get_all(&db, DBPaging::default())?.len(), 3);
    assert_eq!(PropEdge::get_all(&db, DBPaging::default())?.len(), 8);       // 9 edges deleted by cascade on nodes
    println!("Deleting video {}", videos[1].id);

    // delete a video and check that all edges from/to it and its comments are deleted
    Video::delete(&db, &videos[1].id)?;
    assert_eq!(PropEdge::get_all(&db, DBPaging::default())?.len(), 5);  // 1 edge and 2 comments deleted by cascade on video, 2 more edges deleted by cascade on the 2 deleted comments

    Ok(())
}


// ----------------------------------------------------------------------------


#[test]
#[traced_test]
fn test_fixture_state() -> anyhow::Result<()>
{
    let (db, _data_dir, videos, comments, _nodes, _edges) = make_test_db();

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
        assert_eq!(Video::get(&db, &v.id)?.id, v.id);
        let comments = Comment::get_by_video(&db, &v.id, DBPaging::default())?;
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
        assert_eq!(models::Comment::get(&db, &c.id)?.id, c.id);
        assert_eq!(models::Comment::get(&db, &c.id)?.comment, c.comment);
    }

    // Check that we can get videos by user
    assert_eq!(models::Video::get_by_user(&db, "user.num1", DBPaging::default())?.len(), 3);
    assert_eq!(models::Video::get_by_user(&db, "user.num2", DBPaging::default())?.len(), 2);
    Ok(())
}


#[test]
#[traced_test]
fn test_comment_delete() -> anyhow::Result<()> {
    let (db, _data_dir, _vid, com, _nodes, _edges) = make_test_db();

    assert_eq!(Comment::get_by_video(&db, &com[1].video_id, DBPaging::default())?.len(), 2, "Video should have 2 comments before deletion");

    // Delete comment #2 and check that it was deleted, and nothing else
    models::Comment::delete(&db, &com[1].id)?;
    for c in com.iter() {
        if c.id == com[1].id {
            assert!(matches!(models::Comment::get(&db, &c.id).unwrap_err() , DBError::NotFound()), "Comment should be deleted");
        } else {
            assert_eq!(models::Comment::get(&db, &c.id)?.id, c.id, "Deletion removed wrong comment(s)");
        }
    }

    // Check that video still has 1 comment
    assert_eq!(Comment::get_by_video(&db, &com[1].video_id, DBPaging::default())?.len(), 1, "Video should have 1 comment left");

    // Delete last, add a new one and check for ID reuse
    models::Comment::delete(&db, &com[6].id)?;
    let c = CommentInsert {
        video_id: com[1].video_id.clone(),
        parent_id: None,
        user_id: com[1].user_id.clone(),
        user_name: "name".to_string(),
        comment: "re-add".to_string(),
        timecode: None,
        drawing: None,
    };
    let new_id = models::Comment::insert(&db, &c)?.id;
    assert_ne!(new_id, com[6].id, "Comment ID was re-used after deletion. This would mix up comment threads in the UI.");
    Ok(())
}

#[test]
#[traced_test]
fn test_rename_video() -> anyhow::Result<()> {
    let (db, _data_dir, _vid, _com, _nodes, _edges) = make_test_db();

    // Rename video #1
    let new_name = "New name";
    Video::rename(&db, "11111", new_name)?;

    // Check that video #1 has new name
    let v = Video::get(&db, &"11111".into())?;
    assert_eq!(v.title, Some(new_name.into()));

    // Check that video #2 still has old name
    let v = Video::get(&db, &"22222".into())?;
    assert_ne!(v.title, Some(new_name.into()));

    Ok(())
}


#[test]
#[traced_test]
fn test_user_messages() -> anyhow::Result<()> {
    let (db, _data_dir, _vid, _com, _nodes, _edges) = make_test_db();

    // Add a message to user #1
    let msgs = [
        MessageInsert {
            user_id: "user.num1".into(),
            message: "message1".into(),
            event_name: "info".into(),
            video_id: Some("HASH0".into()),
            comment_id: None,
            details: "".into(),
            seen: false,
        },
        MessageInsert {
            user_id: "user.num1".into(),
            message: "message2".into(),
            event_name: "oops".into(),
            video_id: Some("HASH0".into()),
            comment_id: None,
            details: "STACKTRACE".into(),
            seen: false,
        },
        MessageInsert {
            user_id: "user.num2".into(),
            message: "message3".into(),
            event_name: "info".into(),
            video_id: None,
            comment_id: None,
            details: "".into(),
            seen: false,
        },
    ];

    let mut new_msgs = vec![];
    for i in 0..msgs.len() {
        let new_msg = Message::insert(&db, &msgs[i])?;
        assert_eq!(new_msg.user_id, msgs[i].user_id);
        assert_eq!(new_msg.message, msgs[i].message);

        let a = serde_json::to_value(Message::get(&db, &new_msg.id)?.to_proto3())?;
        let b = serde_json::to_value(new_msg.to_proto3())?;
        assert_eq!(a,b);

        assert!(!Message::get(&db, &new_msg.id)?.seen);
        new_msgs.push(new_msg);
    }

    // Correctly count messages
    assert_eq!(Message::get_by_user(&db, "user.num1", DBPaging::default())?.len(), 2);
    assert_eq!(Message::get_by_user(&db, "user.num2", DBPaging::default())?.len(), 1);

    // Mark message #2 as seen
    Message::set_seen(&db, new_msgs[1].id, true)?;
    assert!(Message::get(&db, &new_msgs[1].id)?.seen);

    // Delete & recount
    Message::delete(&db, &new_msgs[2].id)?;
    Message::delete(&db, &new_msgs[0].id)?;
    assert_eq!(Message::get_by_user(&db, "user.num1", DBPaging::default())?.len(), 1);
    assert_eq!(Message::get_by_user(&db, "user.num2", DBPaging::default())?.len(), 0);

    Ok(())
}

#[test]
#[traced_test]
fn test_transaction_rollback() -> anyhow::Result<()> {
    let (db, _data_dir, vid, _com, _nodes, _edges) = make_test_db();

    assert_eq!(Video::get_all(&db, DBPaging::default()).unwrap().len(), vid.len());
    begin_transaction(&db.conn()?)?;
    Video::delete(&db, &vid[0].id)?;
    assert_eq!(Video::get_all(&db, DBPaging::default()).unwrap().len(), vid.len()-1);
    rollback_transaction(&db.conn()?)?;
    assert_eq!(Video::get_all(&db, DBPaging::default()).unwrap().len(), vid.len());

    Ok(())
}

#[test]
#[traced_test]
fn test_transaction_commit() -> anyhow::Result<()> {
    let (db, _data_dir, vid, _com, _nodes, _edges) = make_test_db();

    assert_eq!(Video::get_all(&db, DBPaging::default()).unwrap().len(), vid.len());
    begin_transaction(&db.conn()?)?;
    Video::delete(&db, &vid[0].id)?;
    assert_eq!(Video::get_all(&db, DBPaging::default()).unwrap().len(), vid.len()-1);
    commit_transaction(&db.conn()?)?;
    assert_eq!(Video::get_all(&db, DBPaging::default()).unwrap().len(), vid.len()-1);

    Ok(())
}