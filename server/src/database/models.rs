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

#[derive(Serialize, Deserialize, Debug, Associations, Queryable, Selectable, Identifiable, QueryId, Clone)]
#[diesel(belongs_to(Video, foreign_key = video_id))]
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

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable, Associations, Clone)]
#[diesel(belongs_to(Video, foreign_key = ref_video_id))]
#[diesel(belongs_to(Comment, foreign_key = ref_comment_id))]
pub struct Message {
    pub id: i32,
    pub user_id: String,

    #[serde(with = "ts_seconds")]
    pub created: chrono::NaiveDateTime,

    pub seen: bool,
    pub ref_video_id: Option<String>,
    pub ref_comment_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Insertable, Clone, Associations)]
#[diesel(table_name = messages)]
#[diesel(belongs_to(Video, foreign_key = ref_video_id))]
#[diesel(belongs_to(Comment, foreign_key = ref_comment_id))]
pub struct MessageInsert {
    pub user_id: String,
    pub seen: bool,
    pub ref_video_id: Option<String>,
    pub ref_comment_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}

// -------------------------------------------------------
// Graph structures
// -------------------------------------------------------
#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable, QueryId, Clone)]
#[diesel(table_name = prop_edges)]
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

#[derive(Serialize, Deserialize, Default, Debug, Insertable)]
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



#[derive(Serialize, Deserialize, Debug, Queryable, Selectable, Identifiable, QueryId, Clone)]
#[diesel(table_name = prop_nodes)]
#[diesel(primary_key(id))]
pub struct PropNode {
    pub id: i32,
    pub node_type: String,
    pub body: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Insertable)]
#[diesel(table_name = prop_nodes)]
pub struct PropNodeInsert {
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

// -------------------------------------------------------
// VIEW (stored statement) mappings for queries
// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_videos_pointing_to_node)]
#[diesel(belongs_to(PropNode, foreign_key = node_id))]
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(primary_key(node_id, video_id, edge_type, edge_sibling_id))]
pub struct ViewVideosPointingToNode {
    pub node_id: i32,
    pub node_type: String,
    pub node_body: Option<String>,

    pub edge_type: String,
    pub edge_body: Option<String>,
    pub edge_sort_order: f32,
    pub edge_sibling_id: i32,

    pub video_id: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,
    pub video_owner: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Identifiable, Selectable, QueryId, Associations)]
#[diesel(table_name = view_nodes_pointing_to_video)]
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(belongs_to(PropNode, foreign_key = node_id))]
#[diesel(primary_key(video_id, node_id, edge_type, edge_sibling_id))]
pub struct ViewNodesPointingToVideo {
    pub video_id: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,
    pub video_owner: Option<String>,

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
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(primary_key(video_id))]
pub struct ViewVideosWithoutOutgoingEdges {
    pub video_id: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,
    pub video_owner: Option<String>,
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
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(primary_key(video_id, edge_type))]
pub struct ViewVideoCountOutgoingEdges {
    pub video_id: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,
    pub video_owner: Option<String>,

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
#[diesel(belongs_to(Video, foreign_key = video_id))]
#[diesel(primary_key(video_id, edge_type))]
pub struct ViewVideoCountIncomingEdges {
    pub video_id: String,
    pub video_title: Option<String>,
    pub video_duration: Option<f32>,
    pub video_owner: Option<String>,

    pub edge_type: String,
    pub edge_count: i32,
}
