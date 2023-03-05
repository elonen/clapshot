use diesel::{prelude::*, QueryId};
use serde::{Deserialize, Serialize};
use super::schema::*;
use chrono;
use chrono::naive::serde::{ts_seconds, ts_seconds_option};
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
    pub orig_filename: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,

    pub title: Option<String>,
    pub thumb_sheet_dims: Option<String>,   // e.g. "10x10"
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(table_name = videos)]
pub struct VideoInsert {
    pub video_hash: String,
    pub added_by_userid: Option<String>,
    pub added_by_username: Option<String>,
    pub recompression_done: Option<String>,
    pub orig_filename: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,
    pub title: Option<String>,
    pub thumb_sheet_dims: Option<String>,
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

// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Queryable, Selectable, Identifiable, QueryId)]
pub struct Prop {
    pub id: i32,
    pub obj: Option<String>,
    pub key: String,
    pub val: String,
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(table_name = props)]
pub struct PropInsert {
    pub obj: Option<String>,
    pub key: String,
    pub val: String,
}


// -------------------------------------------------------
// JSON serialization helpers
// -------------------------------------------------------

pub type ToJsonResult = Result<serde_json::Value, serde_json::Error>;
pub type ToJsonPostproc<T> = fn(serde_json::Value, &T) -> ToJsonResult;

fn to_json_default_impl<T: Serialize + Sized>(obj: &T, pproc: Option<ToJsonPostproc<T>>) -> ToJsonResult {
    match (serde_json::to_value(&obj)?, pproc) {
        (v, Some(pproc)) => pproc(v, obj),
        (v, None) => Ok(v),
    }
}

pub trait ToJson {
    fn to_json(&self, pproc: Option<ToJsonPostproc<Self>>) -> ToJsonResult
        where Self: Serialize + Sized {
        to_json_default_impl(self, pproc)
    }
}   

impl ToJson for VideoInsert {}
impl ToJson for Comment {}
impl ToJson for CommentInsert {}
impl ToJson for MessageInsert {}
impl ToJson for Prop {}
impl ToJson for PropInsert {}


/// Convert a UTC timestamp to a human-readable string in the local timezone.
pub fn humanize_utc_timestamp(timestamp: &chrono::NaiveDateTime) -> String {
    use chrono::{Local, DateTime};
    let tz = Local::now().offset().clone();
    let local_dt = DateTime::<Local>::from_utc(*timestamp, tz);
    let hours_ago = Local::now().signed_duration_since(local_dt).num_hours();
    if hours_ago < 24 {
        timeago::Formatter::new().convert_chrono(local_dt, Local::now())
    } else {
        local_dt.format("%Y-%m-%d").to_string() // %H:%M
    }
}

impl ToJson for Video {
    fn to_json(&self, pproc: Option<ToJsonPostproc<Video>>) -> ToJsonResult {
        to_json_default_impl(self, pproc).map(|mut v| {
            v["added_time"] = serde_json::Value::String(humanize_utc_timestamp(&self.added_time));
            v
        })
    }
}

impl ToJson for Message {
    fn to_json(&self, pproc: Option<ToJsonPostproc<Message>>) -> ToJsonResult {
        to_json_default_impl(self, pproc).map(|mut v| {
            v["created"] = serde_json::Value::String(humanize_utc_timestamp(&self.created));
            v
        })
    }
}
