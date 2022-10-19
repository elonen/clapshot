use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool};

use tokio::sync::Mutex;

use super::{WsMsgSender, SenderList, SenderListMap, Res};
use crate::database::DB;

/// Lists of all active connections and other server state vars
#[derive (Clone)]
pub struct ServerState {
    pub terminate_flag: Arc<AtomicBool>,
    pub db: Arc<DB>,
    pub videos_dir: PathBuf,
    pub upload_dir: PathBuf,
    user_id_to_senders: SenderListMap,
    video_hash_to_senders: SenderListMap,
}

impl ServerState {

    pub fn new(db: Arc<DB>, videos_dir: &Path, upload_dir: &Path, terminate_flag: Arc<AtomicBool>) -> ServerState {
        ServerState {
            db,
            videos_dir: videos_dir.to_path_buf(),
            upload_dir: upload_dir.to_path_buf(),
            terminate_flag,
            user_id_to_senders: Arc::new(RwLock::new(HashMap::<String, SenderList>::new())),
            video_hash_to_senders: Arc::new(RwLock::new(HashMap::<String, SenderList>::new())),
        }
    }

    /// Register a new sender (API connection) for a user_id. One user can have multiple connections.
    /// Returns a guard that will remove the sender when dropped.
    pub fn register_user_session(&self, user_id: &str, sender: WsMsgSender) -> Box<Mutex<dyn Send>> {
        self.add_sender_to_maplist(user_id, sender, &self.user_id_to_senders)
    }

    /// Send a message to all sessions user_id has open.
    /// Bails out with error if any of the senders fail.
    /// Returns the number of messages sent.
    pub fn send_to_all_user_sessions(&self, user_id: &str, msg: &super::Message) -> Res<u32> {
        let mut total_sent = 0u32;
        let map = self.user_id_to_senders.read().map_err(|e| format!("Sender map poisoned: {}", e))?;
        for sender in map.get(user_id).unwrap_or(&vec![]).iter() {
            sender.send(msg.clone())?;
            total_sent += 1; };
        Ok(total_sent)
    }

    /// Register a new sender (API connection) as a viewer for a video.
    /// One video can have multiple viewers (including the same user, using different connections).
    /// Returns a guard that will remove the sender when dropped.
    pub fn link_session_to_video(&self, video_hash: &str, sender: WsMsgSender) -> Box<Mutex<dyn Send>> {
        self.add_sender_to_maplist(video_hash, sender, &self.video_hash_to_senders)
    }

    /// Send a message to all sessions that are viewing a video.
    /// Bails out with error if any of the senders fail.
    /// Returns the number of messages sent.
    pub fn send_to_all_video_sessions(&self, video_hash: &str, msg: &super::Message) -> Res<u32> {
        let mut total_sent = 0u32;
        let map = self.video_hash_to_senders.read().map_err(|e| format!("Sender map poisoned: {}", e))?;
        for sender in map.get(video_hash).unwrap_or(&vec![]).iter() {
            sender.send(msg.clone())?;
            total_sent += 1; };
        Ok(total_sent)
    }

    // Common implementations for the above add functions.
    fn add_sender_to_maplist(&self, key: &str, sender: WsMsgSender, maplist: &SenderListMap) -> Box<Mutex<dyn Send>> {
        let mut list = maplist.write().unwrap();
        let senders = list.entry(key.to_string()).or_insert(Vec::new());
        senders.push(sender.clone());

        struct Guard { maplist: SenderListMap, sender: WsMsgSender, key: String }
        impl Drop for Guard {
            fn drop(&mut self) {
                if let Ok(mut list) = self.maplist.write() {
                    let senders = list.entry(self.key.to_string()).or_insert(Vec::new());
                    senders.retain(|s| !self.sender.same_channel(&s));
                    if senders.len() == 0 { list.remove(&self.key); }
                } else { tracing::error!("SenderListMap was poisoned! Leaving a dangling API session."); }
            }}
        Box::new(Mutex::new(Guard { maplist: maplist.clone(), sender: sender.clone(), key: key.to_string() }))
    }
}
