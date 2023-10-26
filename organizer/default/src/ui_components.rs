use std::collections::HashMap;
use lib_clapshot_grpc::proto::org::UserSessionData;

use lib_clapshot_grpc::proto::{self, org};

use crate::folder_ops::{FoldeBodyData, get_current_folder_path, fetch_folder_contents};
use lib_clapshot_grpc::proto::org::graph_obj_rel::Rel::ParentIs;

use crate::graph_utils::PARENT_FOLDER_EDGE_TYPE;
use crate::{GrpcServerConn, RpcResult};


#[derive(serde::Serialize, serde::Deserialize)]
pub struct OpenFolderArgs { pub id: String }


/// Popup actions for when the user right-clicks on a listing background.
pub (crate) fn make_custom_actions_map() -> HashMap<String, proto::ActionDef> {
    HashMap::from([
        ("new_folder".into(), make_new_folder_action()),
        ("move_to_parent".into(), make_move_to_parent_action()),
    ])
}

/// Popup actions for when the user right-clicks on a folder item.
fn make_new_folder_action() -> proto::ActionDef {
    proto::ActionDef  {
        ui_props: Some(proto::ActionUiProps {
            label: Some(format!("New folder")),
            icon: Some(proto::Icon {
                src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                    classes: "fa fa-folder-plus".into(), color: None, })),
                ..Default::default()
            }),
            key_shortcut: None,
            natural_desc: Some(format!("Create a new folder")),
            ..Default::default()
        }),
        action: Some(proto::ScriptCall {
            lang: proto::script_call::Lang::Javascript.into(),
            code: r#"
var folder_name = (prompt("Name for the new folder", ""))?.trim();
if (folder_name) { clapshot.callOrganizer("new_folder", {name: folder_name}); }
                "#.trim().into()
        })
    }
}


/// Popup actions for when the user right-clicks on a folder item.
fn make_move_to_parent_action() -> proto::ActionDef {
    proto::ActionDef  {
        ui_props: Some(proto::ActionUiProps {
            label: Some(format!("Move to parent")),
            icon: Some(proto::Icon {
                src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                    classes: "fa fa-arrow-turn-up".into(), color: None, })),
                ..Default::default()
            }),
            key_shortcut: None,
            natural_desc: Some(format!("Move item to parent folder")),
            ..Default::default()
        }),
        action: Some(proto::ScriptCall {
            lang: proto::script_call::Lang::Javascript.into(),
            code: r#"
if (!listingData.parent_folder_id) {
    alert("parent_folder_id missing from listingData.");
    return;
}
var folderId = listingData.parent_folder_id;
var ids = clapshot.itemsToIDs(items);
clapshot.moveToFolder(folderId, ids, listingData);
                "#.trim().into()
        })
    }
}


// ---------------------------------------------------------------------------

/// Build folder view page.
/// Reads folder_path cookie and builds a list of folders and videos in the folder.
pub async fn construct_navi_page(srv: &mut GrpcServerConn, ses: &UserSessionData, cookie_override: Option<String>)
    -> RpcResult<org::ClientShowPageRequest>
{
    let folder_path = get_current_folder_path(srv, &ses, cookie_override).await?;
    assert!(!folder_path.is_empty(), "Folder path should always contain at least the root folder");
    let cur_folder = folder_path.last().unwrap();

    let (folders, videos, edges) = fetch_folder_contents(srv, &cur_folder).await?;

    let mut popup_actions = vec!["popup_rename".into(), "popup_trash".into()];
    let mut listing_data = HashMap::from([ ("folder_id".into(), cur_folder.id.clone()) ]);

    if folder_path.len() > 1 {
        popup_actions.push("move_to_parent".into());
        listing_data.insert("parent_folder_id".into(), folder_path[folder_path.len() - 2].id.clone());
    }

    // Convert folder and video nodes to page items
    let mut folder_page_items = Vec::new();
    for f in folders {
        folder_page_items.push(folder_node_to_page_item(&f, &popup_actions, srv).await);
    }

    let video_page_items: Vec<proto::page_item::folder_listing::Item> = videos.iter().map(|v| {
        proto::page_item::folder_listing::Item {
            item: Some(proto::page_item::folder_listing::item::Item::Video(v.clone())),
            open_action: Some(proto::ScriptCall {
                lang: proto::script_call::Lang::Javascript.into(),
                code: "clapshot.openVideo(items[0].video.id);".into()
            }),
            popup_actions: popup_actions.clone(),
            vis: None,
        }}).collect();

    let mut items: Vec<_> = folder_page_items.into_iter().chain(video_page_items.into_iter()).collect();  // Concatenate folders + video

    // Reorder page items according to edges (that are sorted by sort_order)
    items.sort_by_key(|item| {
            use lib_clapshot_grpc::proto::page_item::folder_listing::item::Item;
            let graph_obj_id = match &item.item {
                Some(Item::Folder(it)) => org::graph_obj::Id::NodeId(it.id.clone()),
                Some(Item::Video(it)) => org::graph_obj::Id::VideoId(it.id.clone()),
                None => org::graph_obj::Id::NodeId("".into()) };
            edges.iter().position(|e| e.from == Some(org::GraphObj { id: Some(graph_obj_id.clone()) })).unwrap_or_default()
        });

    let folder_listing = proto::page_item::FolderListing {
            items,
            allow_reordering: true,
            popup_actions: vec!["new_folder".into()],
            listing_data
        };

    Ok(org::ClientShowPageRequest {
            sid: ses.sid.clone(),
            page_items: if let Some(html) = make_bredcrumbs_html(folder_path) { vec![
                proto::PageItem { item: Some(proto::page_item::Item::Html(html.into())) },
                proto::PageItem { item: Some(proto::page_item::Item::FolderListing(folder_listing)) },
            ]} else { vec![
                proto::PageItem { item: Some(proto::page_item::Item::FolderListing(folder_listing)) }
            ]},
        })
}



/// Helper: convert a folder node to a page item.
async fn folder_node_to_page_item(folder: &org::PropNode, popup_actions: &Vec<String>, srv: &mut GrpcServerConn) -> proto::page_item::folder_listing::Item {
    let mut folder_data: FoldeBodyData = serde_json::from_str(&folder.body.clone().unwrap_or("{}".into())).unwrap_or_default();

    let preview_items = preview_items_for_folder(&mut folder_data, srv, folder).await;

    let f = proto::page_item::folder_listing::Folder {
        id: folder.id.clone(),
        title: if folder_data.name.is_empty() { "<UNNAMED>".into() } else { folder_data.name.clone() },
        preview_items,
    };
    proto::page_item::folder_listing::Item {
        item: Some(proto::page_item::folder_listing::item::Item::Folder(f.clone())),
        open_action: Some(proto::ScriptCall {
            lang: proto::script_call::Lang::Javascript.into(),
            code: format!(r#"clapshot.callOrganizer("open_folder", {{id: "{}"}});"#, f.id),
        }),
        popup_actions: popup_actions.clone(),
        ..Default::default()
    }
}

/// Helper: Make sure folder has preview items.
/// If folder preview cache is None, fetch contained videos from DB and turn them into preview items.
async fn preview_items_for_folder(_folder_data: &mut FoldeBodyData, srv: &mut GrpcServerConn, folder: &org::PropNode)
    -> Vec<proto::page_item::folder_listing::Item>
{
    if true  // folder_data.preview_cache.is_none()
    {
        // Get videos contained in this folder
        let contained_videos = srv.db_get_videos(org::DbGetVideosRequest {
            filter: Some(org::db_get_videos_request::Filter::GraphRel(
                org::GraphObjRel {
                    rel: Some(ParentIs(org::GraphObj { id: Some(org::graph_obj::Id::NodeId(folder.id.clone())) })),
                    edge_type: Some(PARENT_FOLDER_EDGE_TYPE.into()),
                })),
            ..Default::default() }).await;

        match contained_videos {
            Ok(videos) => {
                // Make preview Items from videos
                let mut preview_items = Vec::new();
                for v in videos.into_inner().items {
                    preview_items.push( proto::page_item::folder_listing::Item {
                        item: Some(proto::page_item::folder_listing::item::Item::Video(v)),
                        ..Default::default()
                    });
                }
                return preview_items;
                /*
                // Update database entry
                folder_data.preview_cache = Some(preview_items);
                srv.db_upsert(org::DbUpsertRequest {
                    nodes: vec![ org::PropNode {
                        body: Some(serde_json::to_string(&folder_data).unwrap()),
                        ..folder.clone()
                    }],
                    ..Default::default()
                }).await.unwrap();
                */
            },
            Err(e) => {
                tracing::warn!("Failed to fetch videos for folder {}: {}", folder.id, e);
            }
        }
    }
    return Vec::new();
}

/// Helper: build breadcrumbs html from folder path.
///
/// Returns None if there is only one item in the path (root folder).
fn make_bredcrumbs_html(folder_path: Vec<org::PropNode>) -> Option<String> {
    let mut breadcrumbs: Vec<(String, String)> = folder_path.iter().map(|f| {
            let id = f.id.clone();
            let name = serde_json::from_str::<FoldeBodyData>(&f.body.clone().unwrap_or("{}".into())).unwrap_or_default().name;
            (id, name)
        }).collect();

    if let Some(root) = breadcrumbs.first_mut() { root.1 = "Home".into(); }

    fn format_breadcrumb(id: &String, label: &String, is_last: bool) -> String {
        let args_json = serde_json::to_string(&OpenFolderArgs { id: id.clone() }).unwrap().replace("\"", "'");
        if is_last {
            format!("<strong>{}</stron>", label)
        } else {
            format!(r##"<a style="text-decoration: underline;" href="javascript:clapshot.callOrganizer('open_folder', {});">{}</a>"##, args_json, label)
        }
    }

    let breadcrumbs_html = breadcrumbs.iter().enumerate().map(|(idx, (id, label))| {
            let is_last = idx == breadcrumbs.len() - 1;
            format_breadcrumb(id, label, is_last)
        }).collect::<Vec<_>>().join(" ▶ ");

    if breadcrumbs.len() > 1 { Some(breadcrumbs_html) } else { None }
}


// ---------------------------------------------------------------------------

/*
/// Build folder view page.
/// Reads folder_path cookie and builds a list of folders and videos in the folder.
pub async fn _construct_permission_page(_srv: &mut GrpcServerConn, ses: &UserSessionData)
    -> RpcResult<org::ClientShowPageRequest>
{
    // !!! TEMP: read html from file every time for easy development
    // --> replace with include_str!() when done
    let perms_html = std::fs::read_to_string("/home/jarno/clapshot/organizer/default/html/permission_dlg.html")
        .expect("Failed to read html/permission_dlg.html");
    //     //let perms_html = include_str!("../html/permission_dlg.html");

    Ok(org::ClientShowPageRequest {
        sid: ses.sid.clone(),
        page_items: vec![
            proto::PageItem { item: Some(proto::page_item::Item::Html(perms_html.into())) },
        ],
    })
}
*/
