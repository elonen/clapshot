pub mod grpc_client;
pub mod caller;
pub mod grpc_server;

use std::{num::NonZeroU64, collections::HashMap};

use lib_clapshot_grpc::proto;


/// Convert database time to protobuf3
pub fn datetime_to_proto3(dt: &chrono::NaiveDateTime) -> pbjson_types::Timestamp {
    pbjson_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Convert database Video to protobuf3
pub (crate) fn db_video_to_proto3(v: &crate::database::models::Video, url_base: &str) -> proto::Video {

    let duration = match (v.duration, v.total_frames, &v.fps) {
        (Some(dur), Some(total_frames), Some(fps)) => Some(proto::VideoDuration {
            duration: dur as f64,
            total_frames: total_frames as i64,
            fps: fps.clone(),
        }),
        _ => None,
    };

    let added_by = match (&v.added_by_userid, &v.added_by_username) {
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

    fn parse_sheet_dims(sheet_dims: &str) -> Option<(u32, u32)> {
        let (w, h) = sheet_dims.split_once('x')
            .or_else(|| { tracing::error!("Invalid sheet dimensions: {}", sheet_dims); None })?;
        let err_fn = || { tracing::error!("Invalid dim number(s): {}", sheet_dims); None };
        Some((w.parse::<NonZeroU64>().ok().or_else(err_fn)?.get() as u32,
            h.parse::<NonZeroU64>().ok().or_else(err_fn)?.get() as u32))
    }

    let preview_data = if let Some(sheet_dims) = v.thumb_sheet_dims.clone() {
        if let Some((cols, rows)) = parse_sheet_dims(&sheet_dims) {
            let thumb_sheet = Some(proto::video_preview_data::ThumbSheet {
                url: format!("{}/videos/{}/thumbs/sheet-{}.webp", url_base, &v.video_hash, sheet_dims),
                rows,
                cols,
            });
            Some(proto::VideoPreviewData {
                thumb_url: Some(format!("{}/videos/{}/thumbs/thumb.webp", url_base, &v.video_hash)),
                thumb_sheet,
            }            
        ) } else { None } } else { None };

    proto::Video {
        video_hash: v.video_hash.clone(),
        title: v.title.clone(),
        added_by,
        duration,
        added_time: Some(datetime_to_proto3(&v.added_time)),
        preview_data: preview_data,
        processing_metadata: processing_metadata,
    }
}

pub (crate) fn make_video_popup_actions() -> proto::ClientDefineActionsRequest {

    let rename_video = make_rename_action("video", proto::ApiCall {
        sys: proto::api_call::Subsystem::Server.into(),
        cmd: "rename_video".into(),
        data: HashMap::from([("video_hash".into(), "{{item.videoHash}}".into()), ("new_name".into(), "{{dlg.text}}".into())]),
    });

    proto::ClientDefineActionsRequest {
        actions: HashMap::from([
            ("popup_rename_video".into(), rename_video),
        ]),
        sid: "".into()
    }
}

fn make_rename_action(item_name: &str, api_call: proto::ApiCall) -> proto::ActionDef {
    proto::ActionDef  {
    
        // Define how the popup looks
        ui_props: Some(proto::ActionUiProps {
            label: Some("Rename".into()),
            icon: Some(proto::Icon {
                src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                    classes: "fa fa-edit".into(), color: None, })),
                ..Default::default()
            }),
            key_shortcut: Some("F2".into()),
            natural_desc: Some("Rename selected video".into()),
            ..Default::default()
        }),

        // Show a dialog when the action is clicked
        dlg: Some(proto::Dialog {
            r#type: proto::DialogType::TextInput.into(),
            title: "Rename video".into(),
            desc: Some("Enter new name for video".into()),
            args: HashMap::from([("text".into(), "{{item.title}}".into())]),
        }),

        // only make the calle if user entered a different name
        exec_if: Some("{{not-eq dlg.text item.title}}".into()),

        // then call the server to rename it
        api_call: Some(api_call),
    }
}


/// Convert a list of database Videos to a protobuf3 PageItem (FolderListing)
pub (crate) fn folder_listing_for_videos(videos: &[crate::database::models::Video], url_base: &str) -> proto::PageItem {
    let videos: Vec<proto::page_item::folder_listing::Item> = videos.iter().map(|v| {
            proto::page_item::folder_listing::Item {
                item: Some(proto::page_item::folder_listing::item::Item::Video(db_video_to_proto3(v, url_base))),
                open_action: Some(proto::ApiCall {
                    sys: proto::api_call::Subsystem::Server.into(),
                    cmd: "open_video".into(),
                    data: HashMap::from([("video_hash".into(), v.video_hash.clone().into())]),
                }),
                ..Default::default()
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
