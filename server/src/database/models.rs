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
    pub orig_filename: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,

    pub title: Option<String>,
    pub thumb_sheet_dims: Option<String>,
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

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable, Associations)]
#[diesel(belongs_to(Video, foreign_key = ref_video_hash))]
#[diesel(belongs_to(Comment, foreign_key = ref_comment_id))]
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

#[derive(Serialize, Deserialize, Debug, Default, Insertable, Clone, Associations)]
#[diesel(table_name = messages)]
#[diesel(belongs_to(Video, foreign_key = ref_video_hash))]
#[diesel(belongs_to(Comment, foreign_key = ref_comment_id))]
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
// Graph structures
// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Insertable, Clone)]
#[diesel(table_name = prop_edges)]
pub struct PropEdge {
    pub id: i32,

    pub from_video: Option<String>,
    pub from_comment: Option<String>,
    pub from_node: Option<i32>,

    pub to_video: Option<String>,
    pub to_comment: Option<String>,
    pub to_node: Option<i32>,

    pub edge_type: String,
    pub body: Option<String>,

    pub sort_order: Option<f32>,
    pub sibling_id: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Default, Insertable, Clone)]
#[diesel(table_name = prop_nodes)]
pub struct PropNode {
    pub id: i32,
    pub node_type: String,
    pub body: Option<String>,
}

// -------------------------------------------------------
// Serialization helpers
// -------------------------------------------------------

pub fn humanize_utc_timestamp(timestamp: &chrono::NaiveDateTime) -> String {
    let added_time: chrono::DateTime<chrono::Utc> = chrono::Utc.from_utc_datetime(timestamp);
    let time_ago_str = timeago::Formatter::new().convert_chrono(added_time, chrono::Local::now());
    time_ago_str
} 

pub fn to_json<T: serde::Serialize>(t: &T) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(&t)
}

impl Video {
    pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        to_json(&self).map(|mut v| {
            v["added_time"] = serde_json::Value::String(humanize_utc_timestamp(&self.added_time));
            v
        })
    }
}

impl VideoInsert { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }
impl Comment { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }
impl CommentInsert { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }

impl Message { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
    to_json(&self).map(|mut v| {
        v["created"] = serde_json::Value::String(humanize_utc_timestamp(&self.created));
        v
    })
}}

impl MessageInsert { pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> { to_json(&self) } }


// -------------------------------------------------------
// VIEW (stored statement) mappings for queries
// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_videos_pointing_to_node)]
#[diesel(belongs_to(PropNode, foreign_key = node_id))]
#[diesel(belongs_to(Video, foreign_key = video_hash))]
#[diesel(primary_key(node_id, video_hash, edge_type, edge_sibling_id))]
pub struct ViewVideosPointingToNode {
    pub node_id: i32,
    pub node_type: String,
    pub node_body: Option<String>,

    pub edge_type: String,
    pub edge_body: Option<String>,
    pub edge_sort_order: f32,
    pub edge_sibling_id: i32,

    pub video_hash: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_nodes_pointing_to_video)]
#[diesel(belongs_to(Video, foreign_key = video_hash))]
#[diesel(belongs_to(PropNode, foreign_key = node_id))]
#[diesel(primary_key(video_hash, node_id, edge_type, edge_sibling_id))]
pub struct ViewNodesPointingToVideo {
    pub video_hash: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,

    pub edge_type: String,
    pub edge_body: Option<String>,
    pub edge_sort_order: f32,
    pub edge_sibling_id: i32,

    pub node_id: i32,
    pub node_type: String,
    pub node_body: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId)]
#[diesel(table_name = view_nodes_pointing_to_node)]
#[diesel(primary_key(to_node_id, from_node_id, edge_type, edge_sibling_id))]
pub struct ViewNodesPointingToNode {
    pub to_node_id: i32,
    pub to_node_type: String,
    pub to_node_body: Option<String>,

    pub edge_type: String,
    pub edge_body: Option<String>,
    pub edge_sort_order: f32,
    pub edge_sibling_id: i32,

    pub from_node_id: i32,
    pub from_node_type: String,
    pub from_node_body: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_nodes_without_outgoing_edges)]
#[diesel(belongs_to(PropNode, foreign_key = id))]
#[diesel(primary_key(id))]
pub struct ViewNodesWithoutOutgoingEdges {
    pub id: i32,
    pub node_type: String,
    pub node_body: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_videos_without_outgoing_edges)]
#[diesel(belongs_to(Video, foreign_key = video_hash))]
#[diesel(primary_key(video_hash))]
pub struct ViewVideosWithoutOutgoingEdges {
    pub video_hash: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_node_count_outgoing_edges)]
#[diesel(belongs_to(PropNode, foreign_key = node_id))]
#[diesel(primary_key(node_id, edge_type))]
pub struct ViewNodeCountOutgoingEdges {
    pub node_id: i32,
    pub node_body: Option<String>,
    pub node_type: String,

    pub edge_type: String,
    pub edge_count: i32,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_video_count_outgoing_edges)]
#[diesel(belongs_to(Video, foreign_key = video_hash))]
#[diesel(primary_key(video_hash, edge_type))]
pub struct ViewVideoCountOutgoingEdges {
    pub video_hash: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,

    pub edge_type: String,
    pub edge_count: i32,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_node_count_incoming_edges)]
#[diesel(belongs_to(PropNode, foreign_key = node_id))]
#[diesel(primary_key(node_id, edge_type))]
pub struct ViewNodeCountIncomingEdges {
    pub node_id: i32,
    pub node_body: Option<String>,
    pub node_type: String,

    pub edge_type: String,
    pub edge_count: i32,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_video_count_incoming_edges)]
#[diesel(belongs_to(Video, foreign_key = video_hash))]
#[diesel(primary_key(video_hash, edge_type))]
pub struct ViewVideoCountIncomingEdges {
    pub video_hash: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,

    pub edge_type: String,
    pub edge_count: i32,
}
