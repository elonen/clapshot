use std::collections::HashMap;
use lib_clapshot_grpc::proto::Icon;
use lib_clapshot_grpc::proto::org::UserSessionData;

use lib_clapshot_grpc::proto::page_item::folder_listing::item::Visualization;
use lib_clapshot_grpc::proto::{self, org};

use crate::folder_ops::{FolderData, get_current_folder_path, fetch_folder_contents};
use crate::{GrpcServerConn, RpcResult};


/// Popup actions for when the user right-clicks on a listing background.
pub (crate) fn make_folder_list_popup_actions() -> HashMap<String, proto::ActionDef> {
    HashMap::from([
        ("new_folder".into(), make_new_folder_action()),
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
var folder_name = (await prompt("Name for the new folder", ""))?.trim();
if (folder_name) {
    await call_organizer("new_folder", {name: folder_name});
}
                "#.into()
        })
    }
}


/// Build folder view page.
/// Reads folder_path cookie and builds a list of folders and videos in the folder.
pub async fn construct_navi_page(srv: &mut GrpcServerConn, ses: &UserSessionData)
    -> RpcResult<org::ClientShowPageRequest>
{
    let folder_path = get_current_folder_path(srv, &ses).await?;

    let breadcrumbs_html = folder_path.iter().map(|f| {
        format!(r##"<a href="#" onclick="clapshot.navigatePage({{id: '{}'}}); return false;">{}</a>"##, f.id, f.body.clone().unwrap_or("UNNAMED".into()))
    }).collect::<Vec<_>>().join(" &gt; ");

    let (folder_items, videos) = if !folder_path.is_empty()
    {
        let (folders, videos) = fetch_folder_contents(srv, &folder_path.last().unwrap()).await?;
        let folder_items = folders.into_iter().map(|f| folder_node_to_page_item(&f)).collect::<Vec<_>>();
        (folder_items, videos)
    }
    else
    {
        // Show videos without an explicit parent folder in the root
        let orphan_videos = srv.db_get_videos(org::DbGetVideosRequest {
                filter: Some(org::db_get_videos_request::Filter::GraphRel(
                    org::GraphObjRel {
                        rel: Some(org::graph_obj_rel::Rel::Parentless(proto::Empty {})),
                        edge_type: Some("parent_folder".into()),
                    })),
                ..Default::default()
            }).await?.into_inner().items;

        // TODO: show projects as folders in the root
        let project_folders_placeholder = vec![
            proto::page_item::folder_listing::Item {
                item: Some(proto::page_item::folder_listing::item::Item::Folder(
                    proto::page_item::folder_listing::Folder {
                        id: "project:PLACEHOLDER_TEST_PROJECT_ID".into(),
                        title: "Placeholder Test Project".into(),
                        preview_items: vec![],
                })),
                popup_actions: vec!(),
                open_action: Some(proto::ScriptCall {
                    lang: proto::script_call::Lang::Javascript.into(),
                    code: r#"await call_server("open_folder", {id: f.id});"#.into()
                }),
                vis: Some(Visualization {
                    base_color: None,
                    icon:Some( Icon {
                        src: Some(proto::icon::Src::FaClass(proto::icon::FaClass {
                            classes: "fa fa-film".into(),
                            color: None,
                        })),
                        size: None,
                    })
                })
            }
        ];
        (project_folders_placeholder, orphan_videos)
    };


    let video_page_items: Vec<proto::page_item::folder_listing::Item> = videos.iter().map(|v| {
        proto::page_item::folder_listing::Item {
            item: Some(proto::page_item::folder_listing::item::Item::Video(v.clone())),
            open_action: Some(proto::ScriptCall {
                lang: proto::script_call::Lang::Javascript.into(),
                code: r#"await call_server("open_video", {id: items[0].video.id});"#.into()
            }),
            popup_actions: vec!["popup_rename".into(), "popup_trash".into()],
            vis: None,
        }
    }).collect();

    let items = folder_items.into_iter().chain(video_page_items.into_iter()).collect();

    Ok(org::ClientShowPageRequest {
        sid: ses.sid.clone(),
        page_items: vec![
            proto::PageItem { item: Some(proto::page_item::Item::Html(breadcrumbs_html)) },
            proto::PageItem { item: Some(proto::page_item::Item::FolderListing( proto::page_item::FolderListing {
                items,
                allow_reordering: true,
                popup_actions: vec!["new_folder".into()],
            }
            ))},
        ],
    })
}



/// Helper: convert a folder node to a page item.
fn folder_node_to_page_item(folder: &org::PropNode) -> proto::page_item::folder_listing::Item {
    let folder_data = serde_json::from_str::<FolderData>(&folder.body.clone().unwrap_or("{}".into())).unwrap_or_default();
    let f = proto::page_item::folder_listing::Folder {
        id: folder.id.clone(),
        title: if folder_data.name.is_empty() { "UNNAMED".into() } else { folder_data.name.clone() },
        preview_items: folder_data.preview_cache,
    };
    proto::page_item::folder_listing::Item {
        item: Some(proto::page_item::folder_listing::item::Item::Folder(f.clone())),
        open_action: Some(proto::ScriptCall {
            lang: proto::script_call::Lang::Javascript.into(),
            code: r#"await call_server("open_folder", {id: f.id});"#.into()
        }),
        ..Default::default()
    }
}



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
