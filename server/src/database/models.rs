use diesel::{prelude::*, QueryId};
use serde::{Deserialize, Serialize};
use super::schema::*;
use chrono;
use chrono::naive::serde::{ts_seconds, ts_seconds_option};
use chrono::TimeZone;
use timeago;

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable, AsChangeset, Clone)]
#[diesel(treat_none_as_null = true)]
pub struct User {
    pub id: String,
    pub name: String,

    #[serde(with = "ts_seconds")]
    pub created: chrono::NaiveDateTime,
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(table_name = users)]
pub struct UserInsert {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Queryable, Selectable, Insertable, Identifiable, QueryId, Clone)]
#[diesel(treat_none_as_null = true)]
#[diesel(table_name = media_types)]
#[diesel(primary_key(id))]
pub struct MediaType {
    pub id: String,
}


#[derive(Serialize, Deserialize, Debug, Queryable, Selectable, Identifiable, QueryId, AsChangeset, Clone)]
#[diesel(treat_none_as_null = true)]
#[diesel(table_name = media_files)]
#[diesel(primary_key(id))]
#[diesel(belongs_to(User, foreign_key=user_id))]
pub struct MediaFile {
    pub id: String,
    pub user_id: String,
    pub media_type: Option<String>,

    #[serde(with = "ts_seconds")]
    pub added_time: chrono::NaiveDateTime,
    pub recompression_done: Option<chrono::NaiveDateTime>,
    pub thumbs_done: Option<chrono::NaiveDateTime>,
    pub has_thumbnail: Option<bool>,
    pub thumb_sheet_cols: Option<i32>,
    pub thumb_sheet_rows: Option<i32>,
    pub orig_filename: Option<String>,
    pub title: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,
    pub default_subtitle_id: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(table_name = media_files)]
#[diesel(belongs_to(User, foreign_key = user_id))]
pub struct MediaFileInsert {
    pub id: String,
    pub user_id: String,
    pub media_type: Option<String>,
    pub recompression_done: Option<chrono::NaiveDateTime>,
    pub thumbs_done: Option<chrono::NaiveDateTime>,
    pub has_thumbnail: Option<bool>,
    pub thumb_sheet_cols: Option<i32>,
    pub thumb_sheet_rows: Option<i32>,
    pub orig_filename: Option<String>,
    pub title: Option<String>,
    pub total_frames: Option<i32>,
    pub duration: Option<f32>,
    pub fps: Option<String>,
    pub raw_metadata_all: Option<String>,
    pub default_subtitle_id: Option<i32>,
}

// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable, Associations, AsChangeset, Clone)]
#[diesel(belongs_to(MediaFile, foreign_key = media_file_id))]
#[diesel(treat_none_as_null = true)]
pub struct Subtitle {
    pub id: i32,
    pub media_file_id: String,
    pub title: String,
    pub language_code: String,
    pub filename: Option<String>,
    pub orig_filename: String,

    #[serde(with = "ts_seconds")]
    pub added_time: chrono::NaiveDateTime,

    pub time_offset: f32,
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(table_name = subtitles)]
#[diesel(belongs_to(MediaFile, foreign_key = media_file_id))]
pub struct SubtitleInsert {
    pub media_file_id: String,
    pub title: String,
    pub language_code: String,
    pub filename: Option<String>,
    pub orig_filename: String,
    pub time_offset: f32,
}

// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Associations, Queryable, Selectable, Identifiable, QueryId, AsChangeset, Clone)]
#[diesel(belongs_to(User, foreign_key = user_id))]
#[diesel(belongs_to(MediaFile, foreign_key = media_file_id))]

#[diesel(treat_none_as_null = true)]
pub struct Comment {
    pub id: i32,
    pub media_file_id: String,
    pub parent_id: Option<i32>,

    #[serde(with = "ts_seconds")]
    pub created: chrono::NaiveDateTime,

    #[serde(with = "ts_seconds_option")]
    pub edited: Option<chrono::NaiveDateTime>,

    pub user_id: Option<String>,
    pub username_ifnull: String,
    pub comment: String,
    pub timecode: Option<String>,
    pub drawing: Option<String>,
    pub subtitle_id: Option<i32>,
    pub subtitle_filename_ifnull: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Insertable)]
#[diesel(belongs_to(MediaFile, foreign_key = media_file_id))]
#[diesel(belongs_to(User, foreign_key = user_id))]
#[diesel(table_name = comments)]
pub struct CommentInsert {
    pub media_file_id: String,
    pub parent_id: Option<i32>,
    pub user_id: Option<String>,
    pub username_ifnull: String,
    pub comment: String,
    pub timecode: Option<String>,
    pub drawing: Option<String>,
    pub subtitle_id: Option<i32>,
    pub subtitle_filename_ifnull: Option<String>,

}

// -------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default, Queryable, Selectable, Identifiable, Associations, AsChangeset, Clone)]
#[diesel(belongs_to(User, foreign_key = user_id))]
#[diesel(belongs_to(MediaFile, foreign_key = media_file_id))]
#[diesel(belongs_to(Comment, foreign_key = comment_id))]
#[diesel(treat_none_as_null = true)]
pub struct Message {
    pub id: i32,
    pub user_id: String,

    #[serde(with = "ts_seconds")]
    pub created: chrono::NaiveDateTime,

    pub seen: bool,
    pub media_file_id: Option<String>,
    pub comment_id: Option<i32>,
    pub subtitle_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Insertable, Clone, Associations, AsChangeset)]
#[diesel(table_name = messages)]
#[diesel(belongs_to(User, foreign_key = user_id))]
#[diesel(belongs_to(MediaFile, foreign_key = media_file_id))]
#[diesel(belongs_to(Comment, foreign_key = comment_id))]
pub struct MessageInsert {
    pub user_id: String,
    pub seen: bool,
    pub media_file_id: Option<String>,
    pub comment_id: Option<i32>,
    pub subtitle_id: Option<i32>,
    pub event_name: String,
    pub message: String,
    pub details: String,
}

// -------------------------------------------------------
// Serialization helpers
// -------------------------------------------------------

pub fn humanize_utc_timestamp(timestamp: &chrono::NaiveDateTime) -> String {
    let added_time: chrono::DateTime<chrono::Utc> = chrono::Utc.from_utc_datetime(timestamp);
    let time_ago_str = timeago::Formatter::new().convert_chrono(added_time, chrono::Local::now());
    time_ago_str
}
