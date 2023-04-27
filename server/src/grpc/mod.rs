pub mod grpc_client;
pub mod caller;
pub mod grpc_server;
pub mod grpc_impl_helpers;
pub mod db_models;

use std::collections::HashMap;
use lib_clapshot_grpc::proto;


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

pub fn proto3_to_datetime(ts: &pbjson_types::Timestamp) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::from_timestamp_opt(ts.seconds, ts.nanos as u32)
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
                item: Some(proto::page_item::folder_listing::item::Item::Video(v.to_proto3(url_base))),
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
