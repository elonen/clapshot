#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::collections::HashMap;
use std::sync::Arc;
use std::str::FromStr;
use lib_clapshot_grpc::proto::client::ClientToServerCmd;
use lib_clapshot_grpc::proto::client::client_to_server_cmd::{OpenNavigationPage, OpenMediaFile, DelMediaFile, RenameMediaFile, EditComment, DelComment, JoinCollab, LeaveCollab, CollabReport, ReorderItems};
use parking_lot::RwLock;
type WsMsg = warp::ws::Message;

type Res<T> = anyhow::Result<T>;
type MsgSender = tokio::sync::mpsc::UnboundedSender<WsMsg>;
type SenderList = Vec<MsgSender>;
type SenderListMap = Arc<RwLock<HashMap<String, SenderList>>>;

use serde_json::json;
use anyhow::{anyhow, bail, Context};

use inflector::Inflector;
use data_url::{DataUrl, mime};
use sha2::{Sha256, Digest};
use hex;

use super::user_session::{self, AuthzTopic, org_authz_with_default};

use super::UserSession;

use crate::api_server::server_state::ServerState;
use crate::api_server::user_session::Topic;
use crate::database::error::DBError;
use crate::database::{models, DB, DbBasicQuery, DbQueryByUser, DbQueryByMediaFile, DBPaging};
use crate::database::schema::comments::drawing;
use crate::{send_user_error, send_user_ok, client_cmd};

use lib_clapshot_grpc::proto;

use proto::org::authz_user_action_request as authz_req;


/// Get media file by ID from DB, or send user error.
/// Return None if media file not found and error was sent, or Some(MediaFile) if found.
async fn get_media_file_or_send_error(media_file_id: Option<&str>, ses: &Option<&mut UserSession>, server: &ServerState) -> Res<Option<models::MediaFile>> {
    let media_file_id = media_file_id.ok_or(anyhow!("media file id missing"))?;

    match models::MediaFile::get(&mut server.db.conn()?, &media_file_id.into()) {
        Err(DBError::NotFound()) => {
            if let Some(ses) = ses {
                send_user_error!(ses.user_id, server, Topic::MediaFile(media_file_id), "No such media file.");
            };
            Ok(None)
        }
        Err(e) => { bail!(e); }
        Ok(v) => { Ok(Some(v)) }
    }
}

// ---------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------

/// Send user a navigation page to browse the files / folders they have (and/or something else, if Organizer handles it).
pub async fn msg_open_navigation_page(data: &OpenNavigationPage , ses: &mut UserSession, server: &ServerState) -> Res<()> {
    org_authz_with_default(&ses.org_session, "list media files", true, server,
        &ses.organizer, true, AuthzTopic::Other(None, authz_req::other_op::Op::ViewHome)).await?;

    // Try to delegate request to Organizer.
    if let Some(org) = &ses.organizer {
        let req = proto::org::NavigatePageRequest {
            ses: Some(ses.org_session.clone()),
            page_id: data.page_id.clone(),
        };
        match org.lock().await.navigate_page(req).await {
            Err(e) => {
                if e.code() == tonic::Code::Unimplemented {
                    tracing::debug!("Organizer doesn't implement navigate_page(). Using default.");
                } else if e.code() == tonic::Code::Aborted {
                    tracing::debug!("Ignoring org.navigate_page() result because it GrpcStatus.ABORTED.");
                } else {
                    tracing::error!(err=?e, "Error in organizer navigate_page() call");
                    anyhow::bail!("{}: {}", e.code(), e.message());
                }
            },
            Ok(res) => {
                let res = res.into_inner();
                server.emit_cmd(
                    client_cmd!(ShowPage, {
                        page_items: res.page_items,
                        page_id: data.page_id.clone(),
                        page_title: res.page_title,
                    }),
                    super::SendTo::UserSession(&ses.sid))?;
                return Ok(());
            }
        }
    }

    // Organizer didn't handle this, so return a default listing.
    let media_files = models::MediaFile::get_by_user(&mut server.db.conn()?, &ses.user_id, DBPaging::default())?;
    let h_txt = if media_files.is_empty() {
        "<h2>You have no media yet.</h2>"
    } else {
        "<h2>All your media files</h2>"
    };
    let heading = proto::PageItem{ item: Some(proto::page_item::Item::Html(h_txt.into()))};
    let listing = crate::grpc::folder_listing_for_media_files(&media_files, &server.url_base);
    let page = vec![heading, listing];

    server.emit_cmd(
        client_cmd!(ShowPage, { page_items: page, page_id: data.page_id.clone(), page_title: Some("Your Files".to_string())}),
        super::SendTo::UserSession(&ses.sid))?;
    Ok(())
}


/// User opens a media file.
/// Send them the media info and all comments related to it.
/// Register the session as a viewer of the file (media_file_session_guard).
pub async fn msg_open_media_file(data: &OpenMediaFile, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(v) = get_media_file_or_send_error(Some(&data.media_file_id), &Some(ses), server).await? {
        org_authz_with_default(&ses.org_session,
            "open media file", true, server, &ses.organizer,
            true, AuthzTopic::MediaFile(&v, authz_req::media_file_op::Op::View)).await?;
        send_open_media_file_cmd(server, &ses.sid, &v.id).await?;
        ses.cur_media_file_id = Some(v.id);
    }
    Ok(())
}


pub async fn send_open_media_file_cmd(server: &ServerState, session_id: &str, media_file_id: &str) -> Res<()> {
    server.link_session_to_media_file(session_id, media_file_id)?;
    let conn = &mut server.db.conn()?;
    let v = models::MediaFile::get(conn, &media_file_id.into())?.to_proto3(&server.url_base);
    if v.playback_url.is_none() {
        return Err(anyhow!("No playback file"));
    }
    server.emit_cmd(
        client_cmd!(OpenMediaFile, {media_file: Some(v)}),
        super::SendTo::UserSession(session_id))?;
    let mut cmts = vec![];
    for mut c in models::Comment::get_by_media_file(conn, media_file_id, DBPaging::default())? {
        server.fetch_drawing_data_into_comment(&mut c).await?;
        cmts.push(c.to_proto3());
    }
    server.emit_cmd(
        client_cmd!(AddComments, {comments: cmts}),
        super::SendTo::UserSession(session_id))?;
    Ok(())
}


pub async fn del_media_file_and_cleanup(media_file_id: &str, ses: Option<&mut UserSession>, server: &ServerState) -> Res<()> {
    tracing::info!(media_file_id=media_file_id, user_id=ses.as_ref().map(|u|u.user_id.clone()), "Trashing media file.");

    if let Some(v) = get_media_file_or_send_error(Some(media_file_id), &ses, server).await? {

        // Check authorization against user session, if provided
        if let Some(ses) = &ses {
            let default_perm = ses.user_id == (&v).user_id || ses.is_admin;
            org_authz_with_default(&ses.org_session, "delete media file", true, server, &ses.organizer,
                default_perm, AuthzTopic::MediaFile(&v, authz_req::media_file_op::Op::Delete)).await?;
        }

        models::MediaFile::delete(&mut server.db.conn()?, &v.id)?;
        let mut details = format!("Added by '{}' on {}. Filename was {}.",
            v.user_id.clone(),
            v.added_time,
            v.orig_filename.clone().unwrap_or_default());

        fn backup_media_file_db_row(server: &ServerState, v: &models::MediaFile) -> Res<()> {
            let backup_file = server.media_files_dir.join(v.id.clone()).join("db_backup.json");
            if backup_file.exists() {
                std::fs::remove_file(&backup_file)?;
            }
            let json_str = serde_json::to_string_pretty(&v)?;
            std::fs::write(&backup_file, json_str)?;
            Ok(())
        }

        fn move_media_file_to_trash(server: &ServerState, media_file_id: &str) -> Res<()>
        {
            let media_file_dir = server.media_files_dir.join(media_file_id);
            let trash_dir = server.media_files_dir.join("trash");
            if !trash_dir.exists() {
                std::fs::create_dir(&trash_dir)?;
            }
            let hash_and_datetime = format!("{}_{}", media_file_id, chrono::Utc::now().format("%Y%m%d-%H%M%S"));
            let media_file_trash_dir = trash_dir.join(hash_and_datetime);
            std::fs::rename(&media_file_dir, &media_file_trash_dir)?;
            Ok(())
        }

        let mut cleanup_errors = false;
        if let Err(e) = backup_media_file_db_row(server, &v) {
            details.push_str(&format!(" WARNING: DB row backup failed: {:?}.", e));
            cleanup_errors = true;

        }
        if let Err(e) = move_media_file_to_trash(server, &v.id) {
            details.push_str(&format!(" WARNING: Move to trash failed: {:?}.", e));
            cleanup_errors = true;
        }

        if let Some(ses) = ses {
            let media_type_str = v.media_type.unwrap_or("file".to_string()).to_title_case();
            send_user_ok!(&ses.user_id, &server, Topic::MediaFile(&v.id),
                if !cleanup_errors { format!("{} deleted.", media_type_str) } else { format!("{} deleted, but cleanup had errors.", media_type_str) },
                details, true);
        }
    }
    Ok(())
}


pub async fn msg_del_media_file(data: &DelMediaFile, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    del_media_file_and_cleanup(&data.media_file_id, Some(ses), server).await
}


pub async fn msg_rename_media_file(data: &RenameMediaFile, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(v) = get_media_file_or_send_error(Some(&data.media_file_id), &Some(ses), server).await? {
        let default_perm = ses.user_id == (&v).user_id || ses.is_admin;
        org_authz_with_default(&ses.org_session, "rename media file", true, server, &ses.organizer,
            default_perm, AuthzTopic::MediaFile(&v, authz_req::media_file_op::Op::Rename)).await?;

        let new_name = data.new_name.trim();
        if new_name.is_empty() || !new_name.chars().any(|c| c.is_alphanumeric()) {
            send_user_error!(&ses.user_id, server, Topic::MediaFile(&v.id), "Invalid file name (must have letters/numbers)");
            return Ok(());
        }
        if new_name.len() > 160 {
            send_user_error!(&ses.user_id, server, Topic::MediaFile(&v.id), "Name too long (max 160)");
            return Ok(());
        }
        models::MediaFile::rename(&mut server.db.conn()?, &v.id, new_name)?;
        let media_type_str = v.media_type.unwrap_or("file".to_string()).to_title_case();
        send_user_ok!(&ses.user_id, server, Topic::MediaFile(&v.id), format!("{} renamed.", media_type_str),
            format!("New name: '{}'", new_name), true);
    }
    Ok(())
}


pub async fn msg_add_comment(data: &proto::client::client_to_server_cmd::AddComment, ses: &mut UserSession, server: &ServerState) -> Res<()> {

    let media_file_id = match get_media_file_or_send_error(Some(&data.media_file_id), &Some(ses), server).await? {
        Some(v) => {
            let default_perm = ses.user_id == (&v).user_id || ses.is_admin;
            org_authz_with_default(&ses.org_session, "comment media file", true, server, &ses.organizer,
                default_perm, AuthzTopic::MediaFile(&v, authz_req::media_file_op::Op::Comment)).await?;
            v.id
        },
        None => return Ok(()),
    };

    // Parse drawing data if present and write to file
    let mut drwn = data.drawing.clone();
    if let Some(d) = &drwn {
        if d.starts_with("data:") {

            // Convert data URI to bytes
            let img_uri = DataUrl::process(&d).map_err(|e| anyhow!("Invalid drawing data URI"))?;

            if img_uri.mime_type().type_ != "image" || img_uri.mime_type().subtype != "webp" {
                bail!("Invalid mimetype in drawing: {:?}", img_uri.mime_type())
            }
            let img_data = img_uri.decode_to_vec().map_err(|e| anyhow!("Failed to decode drawing data URI: {:?}", e))?;

            // Make up a filename
            fn sha256hex( data: &[u8] ) -> String {
                let mut hasher = Sha256::new();
                hasher.update(data);
                let result = hasher.finalize();
                hex::encode(result)
            }
            let short_csum = sha256hex(img_data.0.as_ref())[..16].to_string();
            let fname = format!("{}.webp", short_csum);

            // Write to file
            let drawing_path = server.media_files_dir.join(&media_file_id).join("drawings").join(&fname);
            std::fs::create_dir_all(drawing_path.parent().unwrap())
                .map_err(|e| anyhow!("Failed to create drawings dir: {:?}", e))?;
            async_std::fs::write(drawing_path, img_data.0).await.map_err(
                |e| anyhow!("Failed to write drawing file: {:?}", e))?;

            // Replace data URI with filename
            drwn = Some(fname);
        }
    };

    let parent_id = match data.parent_id.as_ref().map(|s| s.parse::<i32>()) {
        Some(Ok(id)) => Some(id),
        Some(Err(_)) => { bail!("Invalid parent_id for comment"); }
        None => None,
    };

    let c = models::CommentInsert {
        media_file_id: media_file_id.to_string(),
        parent_id,
        user_id: Some(ses.user_id.clone()),
        username_ifnull: ses.user_name.clone(),
        comment: data.comment.clone(),
        timecode: data.timecode.clone(),
        drawing: drwn.clone(),
    };
    let c = models::Comment::insert(&mut server.db.conn()?, &c)
        .map_err(|e| anyhow!("Failed to add comment: {:?}", e))?;
    // Send to all clients watching this media file
    ses.emit_new_comment(server, c, super::SendTo::MediaFileId(&media_file_id)).await?;
    Ok(())
}


pub async fn msg_edit_comment(data: &EditComment, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let id = i32::from_str(&data.comment_id)?;
    let conn = &mut server.db.conn()?;

    match models::Comment::get(conn, &id) {
        Ok(old) => {
            let default_perm = Some(&ses.user_id) == old.user_id.as_ref() || ses.is_admin;
            org_authz_with_default(&ses.org_session, "edit comment", true, server, &ses.organizer,
                default_perm, AuthzTopic::Comment(&old, authz_req::comment_op::Op::Edit)).await?;

            let vid = &old.media_file_id;
            models::Comment::edit(conn, id, &data.new_comment)?;

            server.emit_cmd(
                client_cmd!(DelComment, {comment_id: id.to_string()}),
                super::SendTo::MediaFileId(&vid))?;

            let c = models::Comment::get(conn, &id)?;
            ses.emit_new_comment(server, c, super::SendTo::MediaFileId(&vid)).await?;
        }
        Err(DBError::NotFound()) => {
            send_user_error!(&ses.user_id, server, Topic::None, "Failed to edit comment.", "No such comment. Cannot edit.", true);
        }
        Err(e) => { bail!(e); }
    }
    Ok(())
}


pub async fn msg_del_comment(data: &DelComment, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let id = i32::from_str(&data.comment_id)?;
    let conn = &mut server.db.conn()?;
    match models::Comment::get(conn, &id) {
        Ok(cmt) => {
            let default_perm = Some(&ses.user_id) == cmt.user_id.as_ref() || ses.is_admin;
            org_authz_with_default(&ses.org_session, "delete comment", true, server, &ses.organizer,
                default_perm, AuthzTopic::Comment(&cmt, authz_req::comment_op::Op::Delete)).await?;

            let vid = cmt.media_file_id;
            if Some(&ses.user_id) != cmt.user_id.as_ref() && !ses.is_admin {
                send_user_error!(&ses.user_id, server, Topic::MediaFile(&vid), "Failed to delete comment.", "You can only delete your own comments", true);
                return Ok(());
            }
            let all_comm = models::Comment::get_by_media_file(conn, &vid, DBPaging::default())?;
            if all_comm.iter().any(|c| c.parent_id.map(|i| i.to_string()) == Some(id.to_string())) {
                send_user_error!(&ses.user_id, server, Topic::MediaFile(&vid), "Failed to delete comment.", "Comment has replies. Cannot delete.", true);
                return Ok(());
            }
            models::Comment::delete(conn, &id)?;
            server.emit_cmd(
                client_cmd!(DelComment, {comment_id: id.to_string()}),
                super::SendTo::MediaFileId(&vid))?;
        }
        Err(DBError::NotFound()) => {
            send_user_error!(&ses.user_id, server, Topic::None, "Failed to delete comment.", "No such comment. Cannot delete.", true);
        }
        Err(e) => { bail!(e); }
    }
    Ok(())
}


pub async fn msg_list_my_messages(data: &proto::client::client_to_server_cmd::ListMyMessages, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    let conn = &mut server.db.conn()?;
    let msgs = models::Message::get_by_user(conn, &ses.user_id, DBPaging::default())?;
    server.emit_cmd(
        client_cmd!(ShowMessages, { msgs: (&msgs).into_iter().map(|m| m.to_proto3()).collect() }),
        super::SendTo::UserSession(&ses.sid)
    )?;
    for m in msgs {
        if !m.seen { models::Message::set_seen(conn, m.id, true)?; }
    }
    Ok(())
}


pub async fn msg_join_collab(data: &JoinCollab, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(collab_id) = ses.cur_collab_id.clone() {
        if server.sender_is_collab_participant(collab_id.as_str(), &ses.sender) {
            tracing::debug!("{} is already in collab {}. Ignoring double join.", ses.user_name, collab_id);
            return Ok(());
        }
    }
    ses.collab_session_guard = None;
    ses.cur_collab_id = None;

    if let Some(v) = get_media_file_or_send_error(Some(&data.media_file_id), &Some(ses), server).await? {
        org_authz_with_default(&ses.org_session, "join collab", true, server, &ses.organizer,
            true, AuthzTopic::Other(Some(&data.collab_id), authz_req::other_op::Op::JoinCollabSession)).await?;

        match server.link_session_to_collab(&data.collab_id, &v.id, ses.sender.clone()) {
            Ok(csg) => {
                ses.collab_session_guard = Some(csg);
                ses.cur_collab_id = Some(data.collab_id.clone());
                server.emit_cmd(
                    client_cmd!(ShowMessages, { msgs: vec![
                            proto::UserMessage {
                            r#type: proto::user_message::Type::Ok as i32,
                            message: format!("'{}' joined collab", &ses.user_name),
                            ..Default::default()
                        }]
                    }),
                    super::SendTo::Collab(&data.collab_id)
                )?;
            }
            Err(e) => {
                send_user_error!(&ses.user_id, server, Topic::MediaFile(&v.id), format!("Failed to join collab session: {}", e));
            }
        }
    }
    Ok(())
}


pub async fn msg_leave_collab(data: &LeaveCollab, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(collab_id) = &ses.cur_collab_id {
        server.emit_cmd(
            client_cmd!(ShowMessages, { msgs: vec![
                    proto::UserMessage {
                    r#type: proto::user_message::Type::Ok as i32,
                    message: format!("'{}' left collab", &ses.user_name),
                    ..Default::default()
                }]
            }),
            super::SendTo::Collab(&collab_id)
        )?;
        ses.collab_session_guard = None;
        ses.cur_collab_id = None;
    }
    Ok(())
}


pub async fn msg_collab_report(data: &CollabReport, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(collab_id) = &ses.cur_collab_id {
        let ce = client_cmd!(CollabEvent, {
            paused: data.paused,
            r#loop: data.paused,
            seek_time_sec: data.seek_time_sec,
            from_user: ses.user_name.clone(),
            drawing: data.drawing.clone(),
        });
        server.emit_cmd(ce, super::SendTo::Collab(collab_id)).map(|_| ())
    } else {
        send_user_error!(&ses.user_id, server, Topic::None, "Report rejected: no active collab session.");
        return Ok(());
    }
}


pub async fn msg_move_to_folder(data: &proto::client::client_to_server_cmd::MoveToFolder, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(org) = &ses.organizer {
        let req = proto::org::MoveToFolderRequest {
            ses: Some(ses.org_session.clone()),
            dst_folder_id: data.dst_folder_id.clone(),
            ids: data.ids.clone(),
            listing_data: data.listing_data.clone(),
        };
        if let Err(e) = org.lock().await.move_to_folder(req).await {
            if e.code() == tonic::Code::Unimplemented {
                tracing::debug!("Organizer doesn't implement move_to_folder(). Ignoring.");
            } else if e.code() == tonic::Code::Aborted {
                tracing::debug!("Ignoring org.move_to_folder() result because it GrpcStatus.ABORTED.");
            } else {
                tracing::error!(err=?e, "Error in organizer move_to_folder() call");
                anyhow::bail!("Organizer error: {:?}", e);
            }
        }
    } else { send_user_error!(&ses.user_id, server, Topic::None, "No organizer session."); }
    Ok(())
}

pub async fn msg_reorder_items(data: &ReorderItems, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(org) = &ses.organizer {
        let req = proto::org::ReorderItemsRequest {
            ses: Some(ses.org_session.clone()),
            ids: data.ids.clone(),
            listing_data: data.listing_data.clone(),
        };
        if let Err(e) = org.lock().await.reorder_items(req).await {
            if e.code() == tonic::Code::Unimplemented {
                tracing::debug!("Organizer doesn't implement reorder_items(). Ignoring.");
            } else if e.code() == tonic::Code::Aborted {
                tracing::debug!("Ignoring org.reorder_items() result because it GrpcStatus.ABORTED.");
            } else {
                tracing::error!(err=?e, "Error in organizer reorder_items() call");
                anyhow::bail!("Organizer error: {:?}", e);
            }
        }
    } else { send_user_error!(&ses.user_id, server, Topic::None, "No organizer session."); }
    Ok(())
}


pub async fn msg_organizer_cmd(data: &proto::client::client_to_server_cmd::OrganizerCmd, ses: &mut UserSession, server: &ServerState) -> Res<()> {
    if let Some(org) = &ses.organizer {
        let req = proto::org::CmdFromClientRequest {
            ses: Some(ses.org_session.clone()),
            cmd: data.cmd.clone(),
            args: data.args.clone()
        };
        match org.lock().await.cmd_from_client(req).await {
            Err(e) => {
                if e.code() == tonic::Code::Aborted {
                    tracing::debug!("Ignoring org.cmd_from_client() result because it GrpcStatus.ABORTED.");
                } else {
                    tracing::error!(err=?e, "Error in organizer cmd_from_client() call");
                    anyhow::bail!("Organizer error: {:?}", e);
                }
            },
            Ok(res) => { return Ok(()); }
        }
    }
    Ok(())
}



#[derive(thiserror::Error, Debug)]
pub enum SessionClose {
    #[error("User logout")]
    Logout,
}

/// Dispatch a message from client to appropriate handler.
/// Return true if the session should be kept open, or false if it should be closed.
pub async fn msg_dispatch(req: &ClientToServerCmd, ses: &mut UserSession, server: &ServerState) -> Res<bool> {
    use proto::client::client_to_server_cmd::Cmd;
    let res = match req.cmd.as_ref() {
        None => {
            send_user_error!(&ses.user_id, server, Topic::None, format!("Missing command from client: {:?}", req));
            Ok(())
        }
        Some(cmd) => match cmd {
            Cmd::OpenNavigationPage(data) => msg_open_navigation_page(&data, ses, server).await,
            Cmd::OpenMediaFile(data) => msg_open_media_file(&data, ses, server).await,
            Cmd::DelMediaFile(data) => msg_del_media_file(&data, ses, server).await,
            Cmd::RenameMediaFile(data) => msg_rename_media_file(&data, ses, server).await,
            Cmd::AddComment(data) => msg_add_comment(&data, ses, server).await,
            Cmd::EditComment(data) => msg_edit_comment(&data, ses, server).await,
            Cmd::DelComment(data) => msg_del_comment(&data, ses, server).await,
            Cmd::ListMyMessages(data) => msg_list_my_messages(&data, ses, server).await,
            Cmd::JoinCollab(data) => msg_join_collab(&data, ses, server).await,
            Cmd::LeaveCollab(data) => msg_leave_collab(&data, ses, server).await,
            Cmd::CollabReport(data) => msg_collab_report(&data, ses, server).await,
            Cmd::OrganizerCmd(data) => msg_organizer_cmd(&data, ses, server).await,
            Cmd::MoveToFolder(data) => msg_move_to_folder(&data, ses, server).await,
            Cmd::ReorderItems(data) => msg_reorder_items(&data, ses, server).await,
            Cmd::Logout(_) => {
                tracing::info!("logout from client: user={}", ses.user_id);
                return Err(SessionClose::Logout.into());
            },
        },
    };
    if let Err(e) = res {
        // Ignore authz errors, they are already logged
        if let None = e.downcast_ref::<user_session::AuthzError>() {
            let cmd_str = req.cmd.as_ref().map(|c| format!("{:?}", c)).unwrap_or_default();
            tracing::warn!("[{}] '{cmd_str}' failed: {}", ses.sid, e);
            // Assume name is regex '^[a-zA-Z0-9_]+' of cmd_str
            let cmd_name = regex::Regex::new(r"^[a-zA-Z0-9_]+").unwrap().find(&cmd_str).map(|m| m.as_str()).unwrap_or(cmd_str.as_str());
            send_user_error!(&ses.user_id, server, Topic::None, format!("Cmd '{cmd_name}' failed: {e}"));
        }
    }
    Ok(true)
}
