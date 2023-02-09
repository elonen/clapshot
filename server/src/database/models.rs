use diesel::{prelude::*, QueryId};
use serde::{Deserialize, Serialize};
use super::schema::*;
use chrono;
use chrono::naive::serde::{ts_seconds, ts_seconds_option};
use chrono::TimeZone;
use timeago;


#[derive(Serialize, Deserialize, Debug, Queryable, Selectable, Identifiable, QueryId, Clone)]
#[diesel(table_name = videos)]
pub struct Video {
    pub id: i32,
    pub video_hash: String,
    pub added_by_userid: Option<String>,
    pub added_by_username: Option<String>,

    #[serde(with = "ts_seconds")]
    pub added_time: chrono::NaiveDateTime,

    pub recompression_done: Option<String>,
    pub thumb_sheet_dims: Option<String>,
    pub orig_filename: Option<String>,
    pub title: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(table_name = videos)]
pub struct VideoInsert {
    pub video_hash: String,
    pub added_by_userid: Option<String>,
    pub added_by_username: Option<String>,
    pub recompression_done: Option<String>,
    pub thumb_sheet_dims: Option<String>,
    pub orig_filename: Option<String>,
    pub title: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,
}

// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Associations, Queryable, Selectable, Identifiable, QueryId)]
#[diesel(belongs_to(Video, foreign_key = video_hash))]
pub struct Comment {
    pub id: i32,
    pub video_hash: String,
    pub parent_id: Option<i32>,

    #[serde(with = "ts_seconds")]
    pub created: chrono::NaiveDateTime,

    #[serde(with = "ts_seconds_option")]
    pub edited: Option<chrono::NaiveDateTime>,

    pub user_id: String,
    pub username: String,
    pub comment: String,
    pub timecode: Option<String>,
    pub drawing: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(belongs_to(Video, foreign_key = video_hash))]
#[diesel(table_name = comments)]
pub struct CommentInsert {
    pub video_hash: String,
    pub parent_id: Option<i32>,
    pub user_id: String,
    pub username: String,
    pub comment: String,
    pub timecode: Option<String>,
    pub drawing: Option<String>,
}

// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable)]
pub struct Message {
    pub id: i32,
    pub user_id: String,

    #[serde(with = "ts_seconds")]
    pub created: chrono::NaiveDateTime,
    
    pub seen: bool,
    pub ref_video_hash: Option<String>,
    pub ref_comment_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Insertable, Clone)]
#[diesel(table_name = messages)]
pub struct MessageInsert {
    pub user_id: String,
    pub seen: bool,
    pub ref_video_hash: Option<String>,
    pub ref_comment_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}

pub fn to_json<T: serde::Serialize>(t: &T) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(&t)
}

impl Video { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }
impl VideoInsert { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }
impl Comment { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }
impl CommentInsert { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }

impl Message { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
    to_json(&self).map(|mut v| {
        // Turn timestamp into a human-readable timeago string
        let created_utc: chrono::DateTime<chrono::Utc> = chrono::Utc.from_utc_datetime(&self.created);
        let time_ago_str = timeago::Formatter::new().convert_chrono(created_utc, chrono::Local::now());
        v["created"] = serde_json::json!(time_ago_str);
        v
    })
}}

impl MessageInsert { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }
