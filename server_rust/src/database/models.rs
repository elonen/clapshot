use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use super::schema::*;
use chrono;

#[derive(Debug, Queryable, Insertable, Selectable, Identifiable)]
pub struct Video {
    pub id: i32,
    pub video_hash: String,
    pub added_by_userid: Option<String>,
    pub added_by_username: Option<String>,
    pub added_time: chrono::NaiveDateTime,
    pub recompression_done: Option<String>,
    pub orig_filename: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,
}

#[derive(Debug, Associations, Queryable, Insertable, Selectable, Identifiable)]
#[diesel(belongs_to(Video, foreign_key = video_hash))]
pub struct Comment {
    pub id: i32,
    pub video_hash: String,
    pub parent_id: Option<i32>,
    pub created: chrono::NaiveDateTime,
    pub edited: Option<chrono::NaiveDateTime>,
    pub user_id: String,
    pub username: String,
    pub comment: String,
    pub timecode: Option<String>,
    pub drawing: Option<String>,
}

#[derive(Debug, Queryable, Insertable, Selectable, Identifiable)]
pub struct Message {
    pub id: i32,
    pub user_id: String,
    pub created: chrono::NaiveDateTime,
    pub seen: bool,
    pub ref_video_hash: Option<String>,
    pub ref_comment_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}
