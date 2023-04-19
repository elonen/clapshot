use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc};
use parking_lot::{RwLock, MappedRwLockReadGuard, RwLockReadGuard};
use std::sync::atomic::{AtomicBool};

use tokio::sync::Mutex;
use anyhow::anyhow;

use super::user_session::OpaqueGuard;
use super::{WsMsgSender, SenderList, SessionMap, SenderListMap, StringToStringMap, Res, UserSession, SendTo};
use crate::client_cmd;
use crate::database::{DB, models, DbBasicQuery};
use crate::grpc::db_message_insert_to_proto3;
use crate::grpc::grpc_client::OrganizerURI;
use lib_clapshot_grpc::proto;

/// Lists of all active connections and other server state vars
#[derive (Clone)]
pub struct ServerState {
    pub terminate_flag: Arc<AtomicBool>,
    pub db: Arc<DB>,
    pub videos_dir: PathBuf,
    pub upload_dir: PathBuf,
    pub url_base: String,

    sid_to_session: SessionMap,
    user_id_to_senders: SenderListMap,
    video_id_to_senders: SenderListMap,
    collab_id_to_senders: SenderListMap,
    collab_id_to_video_id: StringToStringMap,

    pub organizer_uri: Option<OrganizerURI>,
    pub organizer_has_connected: Arc<AtomicBool>,
}

impl ServerState {

    pub fn new(
        db: Arc<DB>,
         videos_dir: &Path,
         upload_dir: &Path,
         url_base: &str,
         organizer_uri: Option<OrganizerURI>,
         terminate_flag: Arc<AtomicBool>) -> ServerState
    {
        ServerState {
            db,
            videos_dir: videos_dir.to_path_buf(),
            upload_dir: upload_dir.to_path_buf(),
            terminate_flag,
            url_base: url_base.to_string(),
            sid_to_session: Arc::new(RwLock::new(HashMap::<String, UserSession>::new())),
            user_id_to_senders: Arc::new(RwLock::new(HashMap::<String, SenderList>::new())),
            video_id_to_senders: Arc::new(RwLock::new(HashMap::<String, SenderList>::new())),
            collab_id_to_senders: Arc::new(RwLock::new(HashMap::<String, SenderList>::new())),
            collab_id_to_video_id: Arc::new(RwLock::new(HashMap::<String, String>::new())),
            organizer_uri,
            organizer_has_connected: Arc::new(AtomicBool::new(false)),
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
            SendTo::VideoId(video_id) => { self.send_to_all_video_sessions(video_id, &msg) },
            SendTo::MsgSender(sender) => { sender.send(msg)?; Ok(1u32) },
        }
    }

    /// Send a user message to given recipients.
    pub fn push_notify_message(&self, msg: &models::MessageInsert, send_to: SendTo, persist: bool) -> Res<()> {
        let cmd = client_cmd!(ShowMessages, {msgs: vec![db_message_insert_to_proto3(&msg)]});
        let send_res = self.emit_cmd(cmd, send_to);
        if let Ok(sent_count) = send_res {
            if persist {
                models::Message::add(&self.db, &models::MessageInsert {
                    seen: msg.seen || sent_count > 0,
                    ..msg.clone()
                })?;
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

    /// Send a message to all sessions that are collaboratively viewing a video.
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

    /// Register a new sender (API connection) as a viewer for a video.
    /// One video can have multiple viewers (including the same user, using different connections).
    /// Returns a guard that will remove the sender when dropped.
    pub fn link_session_to_video(&self, video_id: &str, sender: WsMsgSender) -> OpaqueGuard {
        self.add_sender_to_maplist(video_id, sender, &self.video_id_to_senders)
    }

    /// Remove video id mappings from all collabs that have no more viewers.
    fn garbage_collect_collab_video_map(&self) {
        let mut map = self.collab_id_to_video_id.write();
        let senders = self.collab_id_to_senders.read();
        map.retain(|collab_id, _| !senders.get(collab_id).unwrap_or(&vec![]).is_empty());
    }

    pub fn sender_is_collab_participant(&self, collab_id: &str, sender: &WsMsgSender) -> bool {
        let senders = self.collab_id_to_senders.read();
        senders.get(collab_id).unwrap_or(&vec![]).iter().any(|s| s.same_channel(sender))
    }

    pub fn link_session_to_collab(&self, collab_id: &str, video_id: &str, sender: WsMsgSender) -> Res<OpaqueGuard> {
        // GC collab video map. (This might not be the optimal way to do this but at least it
        // will keep it from growing indefinitely.)
        self.garbage_collect_collab_video_map();

        // Only the first joiner (creator) of a collab gets to set the video is.
        let mut map = self.collab_id_to_video_id.write();
        if !map.contains_key(collab_id) {
            map.insert(collab_id.to_string(), video_id.to_string());
        } else if map.get(collab_id).unwrap() != video_id {
            return Err(anyhow!("Mismatching video id for pre-existing collab"));
        }
        Ok(self.add_sender_to_maplist(collab_id, sender, &self.collab_id_to_senders))
    }

    /// Send a message to all sessions that are viewing a video.
    /// Bails out with error if any of the senders fail.
    /// Returns the number of messages sent.
    pub fn send_to_all_video_sessions(&self, video_id: &str, msg: &super::Message) -> Res<u32> {
        let mut total_sent = 0u32;
        let map = self.video_id_to_senders.read();
        for sender in map.get(video_id).unwrap_or(&vec![]).iter() {
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
}
