use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use lib_clapshot_grpc::proto::org::OrganizerInfo;
use parking_lot::{RwLock, MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLockReadGuard, RwLockWriteGuard};
use std::sync::atomic::AtomicBool;

use tokio::sync::Mutex;
use anyhow::anyhow;

use base64::{Engine as _, engine::general_purpose as Base64GP};

use super::user_session::OpaqueGuard;
use super::{WsMsgSender, SenderList, SessionMap, SenderListMap, StringToStringMap, Res, UserSession, SendTo};
use crate::client_cmd;
use crate::database::{DB, models, DbBasicQuery};
use crate::grpc::grpc_client::OrganizerURI;
use lib_clapshot_grpc::proto;

/// Lists of all active connections and other server state vars
#[derive (Clone)]
pub struct ServerState {
    pub grpc_srv_listening_flag: Arc<AtomicBool>,
    pub terminate_flag: Arc<AtomicBool>,
    pub db: Arc<DB>,
    pub media_files_dir: PathBuf,
    pub upload_dir: PathBuf,
    pub url_base: String,
    pub default_user: String,

    sid_to_session: SessionMap,
    user_id_to_senders: SenderListMap,
    media_file_id_to_senders: SenderListMap,
    collab_id_to_senders: SenderListMap,
    collab_id_to_media_file_id: StringToStringMap,

    pub organizer_uri: Option<OrganizerURI>,
    pub organizer_has_connected: Arc<AtomicBool>,
    pub organizer_info: Arc<Mutex<Option<OrganizerInfo>>>
}

impl ServerState {

    pub fn new(
        db: Arc<DB>,
        media_files_dir: &Path,
        upload_dir: &Path,
        url_base: &str,
        organizer_uri: Option<OrganizerURI>,
        grpc_srv_listening_flag: Arc<AtomicBool>,
        default_user: String,
        terminate_flag: Arc<AtomicBool>) -> ServerState
    {
        ServerState {
            db,
            media_files_dir: media_files_dir.to_path_buf(),
            upload_dir: upload_dir.to_path_buf(),
            grpc_srv_listening_flag,
            terminate_flag,
            url_base: url_base.to_string(),
            default_user,
            sid_to_session: Arc::new(RwLock::new(HashMap::<String, UserSession>::new())),
            user_id_to_senders: Arc::new(RwLock::new(HashMap::<String, SenderList>::new())),
            media_file_id_to_senders: Arc::new(RwLock::new(HashMap::<String, SenderList>::new())),
            collab_id_to_senders: Arc::new(RwLock::new(HashMap::<String, SenderList>::new())),
            collab_id_to_media_file_id: Arc::new(RwLock::new(HashMap::<String, String>::new())),
            organizer_uri,
            organizer_has_connected: Arc::new(AtomicBool::new(false)),
            organizer_info: Arc::new(Mutex::new(None)),
        }
    }

    /// Get temp reference to a session object by sid.
    pub fn get_session<'a>(&'a self, sid: &str) -> Option<MappedRwLockReadGuard<'a, UserSession>> {
        let lock = self.sid_to_session.read();
        if lock.contains_key(sid) {
            Some(RwLockReadGuard::map(lock, |map| &map[sid]))
        } else {
            None
        }
    }

    pub fn get_session_write<'a>(&'a self, sid: &str) -> Option<MappedRwLockWriteGuard<'a, UserSession>> {
        let lock = self.sid_to_session.write();
        if lock.contains_key(sid) {
            Some(RwLockWriteGuard::map(lock, |map| map.get_mut(sid).unwrap()))
        } else {
            None
        }
    }
    /// Register a new sender (API connection) for a user_id. One user can have multiple connections.
    /// Returns a guard that will remove the sender when dropped.
    pub fn register_user_session(&self, sid: &str, user_id: &str, ses: UserSession) -> OpaqueGuard {
        let guard1 = self.add_sender_to_maplist(user_id, ses.sender.clone(), &self.user_id_to_senders);
        let guard2 = self.add_session_to_sid_map(sid, ses);
        Arc::new(Mutex::new((guard1, guard2)))
    }

    /// Send a message to a specific session.
    /// Returns the number of messages sent (0 or 1).
    pub fn send_to_user_session(&self, sid: &str, msg: &super::Message) -> Res<u32> {
        if let Some(session) = self.get_session(sid) {
            session.sender.send(msg.clone())?;
            Ok(1)
        } else {
            Ok(0)
        }
    }

    /// Send a client command to websocket of given recipient(s)
    pub fn emit_cmd(&self, cmd: proto::client::server_to_client_cmd::Cmd, send_to: SendTo) -> Res<u32>
    {
        let cmd = proto::client::ServerToClientCmd { cmd: Some(cmd) };
        let msg = serde_json::to_value(cmd)?;
        let msg = warp::ws::Message::text(msg.to_string());
        match send_to {
            SendTo::UserSession(sid) => { self.send_to_user_session(&sid, &msg) },
            SendTo::Collab(id) => { self.send_to_all_collab_users(&Some(id.into()), &msg) },
            SendTo::UserId(user_id) => { self.send_to_all_user_sessions(user_id, &msg) },
            SendTo::MediaFileId(media_file_id) => { self.send_to_all_media_file_sessions(media_file_id, &msg) },
            SendTo::MsgSender(sender) => { sender.send(msg)?; Ok(1u32) },
        }
    }

    /// Send a user message to given recipients.
    pub fn push_notify_message(&self, msg: &models::MessageInsert, send_to: SendTo, persist: bool) -> Res<()> {
        let cmd = client_cmd!(ShowMessages, {msgs: vec![msg.to_proto3()]});
        let send_res = self.emit_cmd(cmd, send_to);
        if let Ok(sent_count) = send_res {
            if persist {
                models::Message::insert(&mut self.db.conn()?, &models::MessageInsert {
                    seen: msg.seen || sent_count > 0,
                    ..msg.clone()
                }).map_err(|e| anyhow!("Failed to persist msg: {}", e))?;
            }
        };
        send_res.map(|_| ())
    }

    /// Send a message to all sessions user_id has open.
    /// Bails out with error if any of the senders fail.
    /// Returns the number of messages sent.
    pub fn send_to_all_user_sessions(&self, user_id: &str, msg: &super::Message) -> Res<u32> {
        let mut total_sent = 0u32;
        let map = self.user_id_to_senders.read();
        for sender in map.get(user_id).unwrap_or(&vec![]).iter() {
            sender.send(msg.clone())?;
            total_sent += 1; };
        Ok(total_sent)
    }

    /// Send a message to all sessions that are collaboratively viewing a media file.
    /// Bails out with error if any of the senders fail.
    /// Returns the number of messages sent.
    pub fn send_to_all_collab_users(&self, collab_id: &Option<String>, msg: &super::Message) -> Res<u32> {
        let mut total_sent = 0u32;
        if let Some(collab_id) = collab_id {
            let map = self.collab_id_to_senders.read();
            for sender in map.get(collab_id).unwrap_or(&vec![]).iter() {
                sender.send(msg.clone())?;
                total_sent += 1; };
        }
        Ok(total_sent)
    }

    /// Register a new sender (API connection) as a viewer for a media file.
    /// One file can have multiple viewers (including the same user, using different connections).
    /// Returns a guard that will remove the sender when dropped.
    pub fn link_session_to_media_file(&self, session_id: &str, media_file_id: &str) -> Res<()> {
        let mut map = self.sid_to_session.write();
        let ses = map.get_mut(session_id).ok_or_else(|| anyhow!("Session {} not found", session_id))?;
        let grd: OpaqueGuard = self.add_sender_to_maplist(media_file_id, ses.sender.clone(), &self.media_file_id_to_senders);
        ses.media_session_guard = Some(grd);
        Ok(())
    }

    /// Remove media file id mappings from all collabs that have no more viewers.
    fn garbage_collect_collab_media_file_map(&self) {
        let mut map = self.collab_id_to_media_file_id.write();
        let senders = self.collab_id_to_senders.read();
        map.retain(|collab_id, _| !senders.get(collab_id).unwrap_or(&vec![]).is_empty());
    }

    pub fn sender_is_collab_participant(&self, collab_id: &str, sender: &WsMsgSender) -> bool {
        let senders = self.collab_id_to_senders.read();
        senders.get(collab_id).unwrap_or(&vec![]).iter().any(|s| s.same_channel(sender))
    }

    pub fn link_session_to_collab(&self, collab_id: &str, media_file_id: &str, sender: WsMsgSender) -> Res<OpaqueGuard> {
        // GC collab media file map. (This might not be the optimal way to do this but at least it
        // will keep it from growing indefinitely.)
        self.garbage_collect_collab_media_file_map();

        // Only the first joiner (creator) of a collab gets to set the media file is.
        let mut map = self.collab_id_to_media_file_id.write();
        if !map.contains_key(collab_id) {
            map.insert(collab_id.to_string(), media_file_id.to_string());
        } else if map.get(collab_id).unwrap() != media_file_id {
            return Err(anyhow!("Mismatching media file id for pre-existing collab"));
        }
        Ok(self.add_sender_to_maplist(collab_id, sender, &self.collab_id_to_senders))
    }

    /// Send a message to all sessions that are viewing a media file.
    /// Bails out with error if any of the senders fail.
    /// Returns the number of messages sent.
    pub fn send_to_all_media_file_sessions(&self, media_file_id: &str, msg: &super::Message) -> Res<u32> {
        let mut total_sent = 0u32;
        let map = self.media_file_id_to_senders.read();
        for sender in map.get(media_file_id).unwrap_or(&vec![]).iter() {
            sender.send(msg.clone())?;
            total_sent += 1; };
        Ok(total_sent)
    }

    // Common implementations for the above add functions.
    fn add_sender_to_maplist(&self, key: &str, sender: WsMsgSender, maplist: &SenderListMap) -> OpaqueGuard {
        let mut list = maplist.write();
        let senders = list.entry(key.to_string()).or_insert(Vec::new());
        senders.push(sender.clone());

        struct Guard { maplist: SenderListMap, sender: WsMsgSender, key: String }
        impl Drop for Guard {
            fn drop(&mut self) {
                let mut list = self.maplist.write();
                let senders = list.entry(self.key.to_string()).or_insert(Vec::new());
                senders.retain(|s| !self.sender.same_channel(&s));
                if senders.len() == 0 { list.remove(&self.key); }
            }
        }
        Arc::new(Mutex::new(Guard { maplist: maplist.clone(), sender: sender.clone(), key: key.to_string() }))
    }

    fn add_session_to_sid_map(&self, sid: &str, ses: UserSession) -> OpaqueGuard {
        self.sid_to_session.write().insert(sid.into(), ses);

        struct Guard { map: SessionMap, sid: String }
        impl Drop for Guard {
            fn drop(&mut self) {
                self.map.write().remove(&self.sid);
            }
        }
        Arc::new(Mutex::new(Guard { map: self.sid_to_session.clone(), sid: sid.to_string() }))
    }

    /// Reads the drawing data from disk and encodes it into a data URI, updating the comment's drawing field
    pub async fn fetch_drawing_data_into_comment(&self, c: &mut models::Comment) -> Res<()> {
        if let Some(drawing) = &mut c.drawing {
            if drawing != "" {
                // If drawing is present, read it from disk and encode it into a data URI.
                if !drawing.starts_with("data:") {
                    let path = self.media_files_dir.join(&c.media_file_id).join("drawings").join(&drawing);
                    if path.exists() {
                        let data = tokio::fs::read(path).await?;
                        *drawing = format!("data:image/webp;base64,{}", Base64GP::STANDARD_NO_PAD.encode(&data));
                    } else {
                        tracing::warn!("Drawing file not found for comment: {}", c.id);
                        c.comment += " [DRAWING NOT FOUND]";
                    }
                } else {
                    // If drawing is already a data URI, just use it as is.
                    // This shouldn't happen anymore, but it's here just in case.
                    tracing::warn!("Comment '{}' has data URI drawing stored in DB. Should be on disk.", c.id);
                }
            }
        };
        Ok(())
    }
}
