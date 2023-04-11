use diesel::{prelude::*, QueryId};
use lib_clapshot_grpc::proto;
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

    pub recompression_done: Option<chrono::NaiveDateTime>,
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
    pub recompression_done: Option<chrono::NaiveDateTime>,
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

pub fn humanize_utc_timestamp(timestamp: &chrono::NaiveDateTime) -> String {
    let added_time: chrono::DateTime<chrono::Utc> = chrono::Utc.from_utc_datetime(timestamp);
    let time_ago_str = timeago::Formatter::new().convert_chrono(added_time, chrono::Local::now());
    time_ago_str
}

pub fn proto_msg_type_to_event_name(t: proto::user_message::Type) -> &'static str {
    match t {
        proto::user_message::Type::Ok => "ok",
        proto::user_message::Type::Error => "error",
        proto::user_message::Type::Progress => "progress",
        proto::user_message::Type::VideoUpdated => "video_updated",
    }
}

pub fn msg_event_name_to_proto_msg_type(t: &str) -> proto::user_message::Type {
    match t {
        "ok" => proto::user_message::Type::Ok,
        "error" => proto::user_message::Type::Error,
        "progress" => proto::user_message::Type::Progress,
        "video_updated" => proto::user_message::Type::VideoUpdated,
        _ => proto::user_message::Type::Ok,
    }
}
