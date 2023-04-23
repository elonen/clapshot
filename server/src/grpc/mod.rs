pub mod grpc_client;
pub mod caller;
pub mod grpc_server;

use std::collections::HashMap;

use lib_clapshot_grpc::proto;

use crate::database::models::msg_event_name_to_proto_msg_type;

// Helper macro to simplify creation of ServerToClientCmd messages.
// Prost/Tonic syntax is a bit verbose.
#[macro_export]
macro_rules! client_cmd {
    ($msg:ident, { $($field:ident: $value:expr),* $(,)? }) => {
        proto::client::server_to_client_cmd::Cmd::$msg(
            proto::client::server_to_client_cmd::$msg {
            $($field: $value,)*
        })
    };
}

/// Convert database time to protobuf3
pub fn datetime_to_proto3(dt: &chrono::NaiveDateTime) -> pbjson_types::Timestamp {
    pbjson_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Convert database Video to protobuf3
///
pub (crate) fn db_video_to_proto3(
    v: &crate::database::models::Video,
    url_base: &str
) -> proto::Video
{
    let duration = match (v.duration, v.total_frames, &v.fps) {
        (Some(dur), Some(total_frames), Some(fps)) => Some(proto::VideoDuration {
            duration: dur as f64,
            total_frames: total_frames as i64,
            fps: fps.clone(),
        }),
        _ => None,
    };

    let added_by = match (&v.user_id, &v.user_name) {
        (Some(user_id), user_name) => Some(proto::UserInfo {
            username: user_id.clone(),
            displayname: user_name.clone(),
        }),
        _ => None,
    };

    let processing_metadata = match (&v.orig_filename, &v.recompression_done, &v.raw_metadata_all.clone()) {
        (Some(orig_filename), recompression_done, ffprobe_metadata_all) => Some(proto::VideoProcessingMetadata {
            orig_filename: orig_filename.clone(),
            recompression_done: recompression_done.map(|t| datetime_to_proto3(&t)),
            ffprobe_metadata_all: ffprobe_metadata_all.clone(),
        }),
        _ => None,
    };

    let preview_data = if let (Some(cols), Some(rows)) = (v.thumb_sheet_cols, v.thumb_sheet_rows) {
        let thumb_sheet = Some(proto::video_preview_data::ThumbSheet {
            url: format!("{}/videos/{}/thumbs/sheet-{}x{}.webp", url_base, &v.id, cols, rows),
            rows: rows as u32,
            cols: cols as u32,
        });
        Some(proto::VideoPreviewData {
            thumb_url: Some(format!("{}/videos/{}/thumbs/thumb.webp", url_base, &v.id)),
            thumb_sheet,
        }
        ) } else { None };

    // Use transcoded or orig video?
    let uri = match v.recompression_done {
        Some(_) => Some("video.mp4".into()),
        None => match &v.orig_filename {
            Some(f) => Some(format!("orig/{}", urlencoding::encode(f))),
            None => None
        }};

    let playback_url = uri.map(|uri|
        format!("{}/videos/{}/{}", url_base, &v.id, uri));

    proto::Video {
        id: v.id.clone(),
        title: v.title.clone(),
        added_by,
        duration,
        added_time: Some(datetime_to_proto3(&v.added_time)),
        preview_data: preview_data,
        processing_metadata: processing_metadata,
        playback_url
    }
}



/// Convert database Comment to protobuf3
///
/// Parent is denormalized only one level up
pub(crate) fn db_comment_to_proto3(
    comment: &crate::database::models::Comment
) -> proto::Comment
{
    let user = proto::UserInfo {
        username: comment.user_id.clone(),
        displayname: Some(comment.user_name.clone()),
    };

    let created_timestamp = Some(datetime_to_proto3(&comment.created));
    let edited_timestamp = comment.edited.map(|edited| datetime_to_proto3(&edited));

    proto::Comment {
        id: comment.id.to_string(),
        video_id: comment.video_id.clone(),
        user: Some(user),
        comment: comment.comment.clone(),
        timecode: comment.timecode.clone(),
        parent_id: comment.parent_id.map(|id| id.to_string()),
        created: created_timestamp,
        edited: edited_timestamp,
        drawing: comment.drawing.clone(),
    }
}

pub (crate) fn db_message_to_proto3(
    msg: &crate::database::models::Message
) -> proto::UserMessage
{
    proto::UserMessage {
        id: Some(msg.id.to_string()),
        r#type: msg_event_name_to_proto_msg_type(&msg.event_name.as_str()).into(),
        refs:Some(proto::user_message::Refs {
            video_id: msg.video_id.clone(),
            comment_id: msg.comment_id.map(|id| id.to_string()),
        }),
        message: msg.message.clone(),
        details: if msg.details.is_empty() { None } else { Some(msg.details.clone()) },
        created: Some(datetime_to_proto3(&msg.created)),
        seen: msg.seen
    }
}

pub (crate) fn db_message_insert_to_proto3(
    msg: &crate::database::models::MessageInsert
) -> proto::UserMessage
{
    proto::UserMessage {
        id: None,
        created: None,
        seen: msg.seen,
        r#type: match msg.event_name.as_str() {
            "error" => proto::user_message::Type::Error,
            "progress" => proto::user_message::Type::Progress,
            "video_updated" => proto::user_message::Type::VideoUpdated,
            _ => proto::user_message::Type::Ok,
        }  as i32,
        refs:Some(proto::user_message::Refs {
            video_id: msg.video_id.clone(),
            comment_id: msg.comment_id.map(|id| id.to_string()),
        }),
        message: msg.message.clone(),
        details: if msg.details.is_empty() { None } else { Some(msg.details.clone()) },
    }
}



pub (crate) fn make_video_popup_actions() -> HashMap<String, proto::ActionDef> {
    HashMap::from([
        ("popup_rename".into(), make_rename_action()),
        ("popup_trash".into(), make_trash_action()),
    ])
}

fn make_rename_action() -> proto::ActionDef {
    proto::ActionDef  {
        ui_props: Some(proto::ActionUiProps {
            label: Some(format!("Rename")),
            icon: Some(proto::Icon {
                src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                    classes: "fa fa-edit".into(), color: None, })),
                ..Default::default()
            }),
            key_shortcut: Some("F2".into()),
            natural_desc: Some(format!("Rename selected items")),
            ..Default::default()
        }),
        action: Some(proto::ScriptCall {
            lang: proto::script_call::Lang::Javascript.into(),
            code: r#"
var it = items[0];
if (!it.video) {
    await alert("Non-video rename not implemented (no Organizer).");
    return;
}
var old_name = it.video.title;
var new_name = (await prompt("Rename item", old_name))?.trim();
if (new_name && new_name != old_name) {
    await call_server("rename_video", {id: it.video.id, new_name: new_name});
}
                "#.into()
        })
    }
}

fn make_trash_action() -> proto::ActionDef {
    proto::ActionDef  {
            ui_props: Some(proto::ActionUiProps {
                label: Some(format!("Trash")),
                icon: Some(proto::Icon {
                    src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                        classes: "fa fa-trash".into(), color: None, })),
                    ..Default::default()
                }),
                key_shortcut: Some("Del".into()),
                natural_desc: Some(format!("Trash selected items")),
                ..Default::default()
            }),
            action: Some(proto::ScriptCall {
                lang: proto::script_call::Lang::Javascript.into(),
                code: r#"
var msg = (items.length == 1)
    ? "Are you sure you want to trash '" + items[0].video?.title + "'?"
    : "Are you sure you want to trash ALL selected items?";
if (await confirm(msg)) {
    for (var i = 0; i < items.length; i++) {
        var it = items[i];
        if (it.video) {
            await call_server("del_video", {id: it.video.id});
        } else {
            await alert("Non-video trash not implemented (no Organizer).");
        }
    }
}
                    "#.into()
            })
    }
}



/// Convert a list of database Videos to a protobuf3 PageItem (FolderListing)
pub (crate) fn folder_listing_for_videos(videos: &[crate::database::models::Video], url_base: &str) -> proto::PageItem {
    let videos: Vec<proto::page_item::folder_listing::Item> = videos.iter().map(|v| {
            proto::page_item::folder_listing::Item {
                item: Some(proto::page_item::folder_listing::item::Item::Video(db_video_to_proto3(v, url_base))),
                open_action: Some(proto::ScriptCall {
                    lang: proto::script_call::Lang::Javascript.into(),
                    code: r#"await call_server("open_video", {id: items[0].video.id});"#.into()
                }),
                popup_actions: vec!["popup_rename".into(), "popup_trash".into()],
                vis: None,
            }
        }).collect();
    /*
    let folders = vec![
        proto::page_item::folder_listing::Item {
            item: Some(proto::page_item::folder_listing::item::Item::Folder(
                proto::page_item::folder_listing::Folder {
                    id: "12345".into(),
                    title: "Test Folder".into(),
                    preview_items: videos.clone() })),
            ..Default::default()
        },
    ];
    let items = folders.into_iter().chain(videos.into_iter()).collect();
    */

    proto::PageItem {
        item: Some(proto::page_item::Item::FolderListing(
            proto::page_item::FolderListing {
                items: videos
        })),
    }
}
