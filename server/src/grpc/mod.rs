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

// Proto3 objects use string for many IDs that are integers in DB. Helper to convert them.
#[macro_export]
macro_rules! str_to_i32_or_tonic_error {
    ($r:expr) => { $r.parse::<i32>().map_err(|e| tonic::Status::invalid_argument(format!("Could not parse {} as int: {}", stringify!($r), e))) };
}
#[macro_export]
macro_rules! optional_str_to_i32_or_tonic_error {
    ($r:expr) => { $r.as_ref().map(|v| v.parse::<i32>().map_err(|e| tonic::Status::invalid_argument(format!("Could not parse {} as int: {}", stringify!($r), e)))).transpose() };
}



/// Convert database time to protobuf3
pub fn datetime_to_proto3(dt: &chrono::NaiveDateTime) -> pbjson_types::Timestamp {
    pbjson_types::Timestamp {
        seconds: dt.and_utc().timestamp(),
        nanos: dt.and_utc().timestamp_subsec_nanos() as i32,
    }
}

pub fn proto3_to_datetime(ts: &pbjson_types::Timestamp) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32).map(|dt| dt.naive_utc())
}

pub (crate) fn make_media_file_popup_actions() -> HashMap<String, proto::ActionDef> {
    HashMap::from([
        ("popup_builtin_rename".into(), make_builtin_rename_action()),
        ("popup_builtin_trash".into(), make_builting_trash_action()),
    ])
}

fn make_builtin_rename_action() -> proto::ActionDef {
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
var it = _action_args.selected_items[0];
var old_name = it.mediaFile?.title || it.folder?.title;
var new_name = (prompt("Rename item", old_name))?.trim();
if (new_name && new_name != old_name) {
    if (it.mediaFile) {
        clapshot.renameMediaFile(it.mediaFile.id, new_name);
    } else if (it.folder) {
        clapshot.callOrganizer("rename_folder", {id: it.folder.id, new_name: new_name});
    } else {
        alert("Unknown item type in rename action. Please report this bug.");
    }
}
                "#.trim().into()
        })
    }
}

fn make_builting_trash_action() -> proto::ActionDef {
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
var items = _action_args.selected_items;

var msg = "Are you sure you want to trash this item?";
if (items.length == 1) {
    if (items[0].mediaFile) {
        msg = "Are you sure you want to trash '" + items[0].mediaFile?.title + "'?";
    } else if (items[0].folder) {
        msg = "Are you sure you want to trash folder '" + items[0].folder?.title + "' and ALL CONTENTS?";
    }
} else {
    msg = "Are you sure you want to trash ALL selected items?";
}
if (confirm(msg)) {
    for (var i = 0; i < items.length; i++) {
        var it = items[i];
        if (it.mediaFile) {
            clapshot.delMediaFile(it.mediaFile.id);
        } else if (it.folder) {
            clapshot.callOrganizer("trash_folder", {id: it.folder.id});
        } else {
            alert("Unknown item type in trash action. Please report this bug.");
        }
    }
}
                    "#.trim().into()
            })
    }
}



/// Convert a list of database MediaFiles to a protobuf3 PageItem (FolderListing)
pub (crate) fn folder_listing_for_media_files(media_files: &[proto::MediaFile]) -> proto::PageItem {
    let media_files: Vec<proto::page_item::folder_listing::Item> = media_files.iter().map(|v| {
            proto::page_item::folder_listing::Item {
                item: Some(proto::page_item::folder_listing::item::Item::MediaFile(v.clone())),
                open_action: Some(proto::ScriptCall {
                    lang: proto::script_call::Lang::Javascript.into(),
                    code: format!("clapshot.openMediaFile(\"{}\")", v.id).into()
                }),
                popup_actions: vec!["popup_builtin_rename".into(), "popup_builtin_trash".into()],
                vis: if v.preview_data.as_ref().and_then(|pv| pv.thumb_url.as_ref()).is_some() { None } else {
                    // If no thumbnail, show an icon based on media type instead
                    Some(proto::page_item::folder_listing::item::Visualization {
                        icon: Some(proto::Icon {
                            src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                                classes: match v.media_type.as_str() {
                                    "audio" => "fas fa-volume-high",
                                    "image" => "fas fa-image",
                                    "video" => "fas fa-video",
                                    _ => "fa fa-circle-question",
                                }.into(), color: None, })),
                            ..Default::default()
                        }),
                        ..Default::default()
                    })
                },
            }
        }).collect();

    proto::PageItem {
        item: Some(proto::page_item::Item::FolderListing(
            proto::page_item::FolderListing {
                items: media_files,
                allow_reordering: false,
                allow_upload: true,
                ..Default::default()
        })),
    }
}
