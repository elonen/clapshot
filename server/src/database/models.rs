use diesel::{prelude::*, QueryId};
use serde::{Deserialize, Serialize};
use super::schema::*;
use chrono;
use chrono::naive::serde::{ts_seconds, ts_seconds_option};
use chrono::TimeZone;
use timeago;

#[derive(Serialize, Deserialize, Debug, Queryable, Selectable, Identifiable, QueryId, AsChangeset, Clone)]
#[diesel(treat_none_as_null = true)]
#[diesel(table_name = videos)]
pub struct Video {
    pub id: String,
    pub user_id: Option<String>,
    pub user_name: Option<String>,

    #[serde(with = "ts_seconds")]
    pub added_time: chrono::NaiveDateTime,
    pub recompression_done: Option<chrono::NaiveDateTime>,
    pub thumb_sheet_cols: Option<i32>,
    pub thumb_sheet_rows: Option<i32>,
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
    pub id: String,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    pub recompression_done: Option<chrono::NaiveDateTime>,
    pub thumb_sheet_cols: Option<i32>,
    pub thumb_sheet_rows: Option<i32>,
    pub orig_filename: Option<String>,
    pub title: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,
}

// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Associations, Queryable, Selectable, Identifiable, QueryId, AsChangeset, Clone)]
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(treat_none_as_null = true)]
pub struct Comment {
    pub id: i32,
    pub video_id: String,
    pub parent_id: Option<i32>,

    #[serde(with = "ts_seconds")]
    pub created: chrono::NaiveDateTime,

    #[serde(with = "ts_seconds_option")]
    pub edited: Option<chrono::NaiveDateTime>,

    pub user_id: String,
    pub user_name: String,
    pub comment: String,
    pub timecode: Option<String>,
    pub drawing: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(table_name = comments)]
pub struct CommentInsert {
    pub video_id: String,
    pub parent_id: Option<i32>,
    pub user_id: String,
    pub user_name: String,
    pub comment: String,
    pub timecode: Option<String>,
    pub drawing: Option<String>,
}

// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable, Associations, AsChangeset, Clone)]
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(belongs_to(Comment, foreign_key = comment_id))]
#[diesel(treat_none_as_null = true)]
pub struct Message {
    pub id: i32,
    pub user_id: String,

    #[serde(with = "ts_seconds")]
    pub created: chrono::NaiveDateTime,

    pub seen: bool,
    pub video_id: Option<String>,
    pub comment_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Insertable, Clone, Associations, AsChangeset)]
#[diesel(table_name = messages)]
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(belongs_to(Comment, foreign_key = comment_id))]
pub struct MessageInsert {
    pub user_id: String,
    pub seen: bool,
    pub video_id: Option<String>,
    pub comment_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}

// -------------------------------------------------------
// Graph structures
// -------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable, QueryId, AsChangeset, Clone)]
#[diesel(table_name = prop_edges)]
#[diesel(treat_none_as_null = true)]
pub struct PropEdge {
    pub id: i32,

    pub from_video: Option<String>,
    pub from_comment: Option<i32>,
    pub from_node: Option<i32>,

    pub to_video: Option<String>,
    pub to_comment: Option<i32>,
    pub to_node: Option<i32>,

    pub edge_type: String,
    pub body: Option<String>,

    pub sort_order: Option<f32>,
    pub sibling_id: Option<i32>,
}

#[derive(Serialize, Deserialize, Default, Debug, Insertable, AsChangeset)]
#[diesel(table_name = prop_edges)]
pub struct PropEdgeInsert {
    pub from_video: Option<String>,
    pub from_comment: Option<i32>,
    pub from_node: Option<i32>,

    pub to_video: Option<String>,
    pub to_comment: Option<i32>,
    pub to_node: Option<i32>,

    pub edge_type: String,
    pub body: Option<String>,

    pub sort_order: Option<f32>,
    pub sibling_id: Option<i32>,
}



#[derive(Serialize, Deserialize, Debug, Queryable, Selectable, Identifiable, QueryId, AsChangeset, Clone)]
#[diesel(table_name = prop_nodes)]
#[diesel(primary_key(id))]
#[diesel(treat_none_as_null = true)]
pub struct PropNode {
    pub id: i32,
    pub node_type: String,
    pub body: Option<String>,
    pub singleton_key: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Insertable, AsChangeset)]
#[diesel(table_name = prop_nodes)]
pub struct PropNodeInsert {
    pub node_type: String,
    pub body: Option<String>,
    pub singleton_key: Option<String>,
}

// -------------------------------------------------------
// Serialization helpers
// -------------------------------------------------------

pub fn humanize_utc_timestamp(timestamp: &chrono::NaiveDateTime) -> String {
    let added_time: chrono::DateTime<chrono::Utc> = chrono::Utc.from_utc_datetime(timestamp);
    let time_ago_str = timeago::Formatter::new().convert_chrono(added_time, chrono::Local::now());
    time_ago_str
}
