use lib_clapshot_grpc::proto;
use crate::database::error::{DBResult, DBError};

use super::{datetime_to_proto3, proto3_to_datetime};


pub fn proto_msg_type_to_event_name(t: proto::user_message::Type) -> &'static str {
    match t {
        proto::user_message::Type::Ok => "ok",
        proto::user_message::Type::Error => "error",
        proto::user_message::Type::Progress => "progress",
        proto::user_message::Type::MediaFileUpdated => "media_file_updated",
        proto::user_message::Type::MediaFileAdded => "media_file_added"
    }
}

pub fn msg_event_name_to_proto_msg_type(t: &str) -> proto::user_message::Type {
    match t {
        "ok" => proto::user_message::Type::Ok,
        "error" => proto::user_message::Type::Error,
        "progress" => proto::user_message::Type::Progress,
        "media_file_updated" => proto::user_message::Type::MediaFileUpdated,
        "media_file_added" => proto::user_message::Type::MediaFileAdded,
        _ => proto::user_message::Type::Ok,
    }
}


// ============================ MediaFile ============================

impl crate::database::models::MediaFile
{
    pub fn from_proto3(v: &proto::MediaFile) -> DBResult<Self>
    {
        Ok(Self {
            id: v.id.clone(),
            user_id: v.user_id.clone(),
            media_type: Some(v.media_type.clone()),
            added_time: v.added_time.as_ref().map(|t| proto3_to_datetime(t)).flatten().ok_or(DBError::Other(anyhow::anyhow!("Bad added_time")))?,
            recompression_done: v.processing_metadata.as_ref().map(|m| m.recompression_done.as_ref().map(|x| proto3_to_datetime(x))).flatten().flatten(),
            thumbs_done: v.processing_metadata.as_ref().map(|m| m.thumbs_done.as_ref().map(|x| proto3_to_datetime(x))).flatten().flatten(),
            has_thumbnail: v.preview_data.as_ref().map(|d| d.thumb_url.is_some()),
            thumb_sheet_cols: v.preview_data.as_ref().map(|d| d.thumb_sheet.as_ref().map(|x| x.cols as i32)).flatten(),
            thumb_sheet_rows: v.preview_data.as_ref().map(|d| d.thumb_sheet.as_ref().map(|x| x.rows as i32)).flatten(),
            orig_filename: v.processing_metadata.as_ref().map(|m| m.orig_filename.clone()),
            title: v.title.clone(),
            total_frames: v.duration.as_ref().map(|d| d.total_frames as i32),
            duration: v.duration.as_ref().map(|d| d.duration as f32),
            fps: v.duration.as_ref().map(|d| d.fps.clone()),
            raw_metadata_all: v.processing_metadata.as_ref().map(|m| m.ffprobe_metadata_all.clone()).flatten(),
        })
    }

    pub fn to_proto3(&self, url_base: &str) -> proto::MediaFile
    {
        let duration = match (self.duration, self.total_frames, &self.fps) {
            (Some(dur), Some(total_frames), Some(fps)) => Some(proto::MediaFileDuration {
                duration: dur as f64,
                total_frames: total_frames as i64,
                fps: fps.clone(),
            }),
            _ => None,
        };
        let processing_metadata = match (&self.orig_filename, &self.recompression_done, &self.raw_metadata_all.clone()) {
            (Some(orig_filename), recompression_done, ffprobe_metadata_all) => Some(proto::MediaFileProcessingMetadata {
                orig_filename: orig_filename.clone(),
                recompression_done: recompression_done.map(|t| datetime_to_proto3(&t)),
                thumbs_done: self.thumbs_done.map(|t| datetime_to_proto3(&t)),
                ffprobe_metadata_all: ffprobe_metadata_all.clone(),
            }),
            _ => None,
        };

        // Make preview data (thumb sheet and/or thumb url)
        let thumb_url = if matches!(self.has_thumbnail, Some(true)) {
            Some(format!("{}/videos/{}/thumbs/thumb.webp", &url_base, &self.id))
        } else { None };

        let thumb_sheet = match (self.thumb_sheet_cols, self.thumb_sheet_rows) {
            (Some(cols), Some(rows)) => Some(proto::media_file_preview_data::ThumbSheet {
                url: format!("{}/videos/{}/thumbs/sheet-{}x{}.webp", &url_base, &self.id, cols, rows),
                rows: rows as u32,
                cols: cols as u32,
            }),
            _ => None
        };

        let preview_data = if thumb_url.is_some() || thumb_sheet.is_some() {
            Some(proto::MediaFilePreviewData { thumb_url, thumb_sheet })
        } else { None };

        // Use transcoded or orig video?
        let orig_uri = match &self.orig_filename {
            Some(f) => Some(format!("orig/{}", urlencoding::encode(f))),
            None => None
        };
        let playback_uri = match self.recompression_done {
            Some(_) => Some("video.mp4".into()),
            None => orig_uri.clone()
        };

        proto::MediaFile {
            id: self.id.clone(),
            title: self.title.clone(),
            media_type: self.media_type.clone().unwrap_or_default(),
            user_id: self.user_id.clone(),
            duration,
            added_time: Some(datetime_to_proto3(&self.added_time)),
            preview_data,
            processing_metadata,
            playback_url: playback_uri.map(|uri| format!("{}/videos/{}/{}", url_base, &self.id, uri)),
            orig_url: orig_uri.map(|uri| format!("{}/videos/{}/{}", url_base, &self.id, uri))
        }
    }
}

impl crate::database::models::MediaFileInsert
{
    pub fn from_proto3(v: &proto::MediaFile) -> DBResult<Self>
    {
        Ok(Self {
            id: v.id.clone(),
            user_id: v.user_id.clone(),
            media_type: Some(v.media_type.clone()),
            recompression_done: v.processing_metadata.as_ref().map(|m| m.recompression_done.as_ref().map(|x| proto3_to_datetime(x))).flatten().flatten(),
            thumbs_done: v.processing_metadata.as_ref().map(|m| m.thumbs_done.as_ref().map(|x| proto3_to_datetime(x))).flatten().flatten(),
            has_thumbnail: v.preview_data.as_ref().map(|d| d.thumb_url.is_some()),
            thumb_sheet_cols: v.preview_data.as_ref().map(|d| d.thumb_sheet.as_ref().map(|x| x.cols as i32)).flatten(),
            thumb_sheet_rows: v.preview_data.as_ref().map(|d| d.thumb_sheet.as_ref().map(|x| x.rows as i32)).flatten(),
            orig_filename: v.processing_metadata.as_ref().map(|m| m.orig_filename.clone()),
            title: v.title.clone(),
            total_frames: v.duration.as_ref().map(|d| d.total_frames as i32),
            duration: v.duration.as_ref().map(|d| d.duration as f32),
            fps: v.duration.as_ref().map(|d| d.fps.clone()),
            raw_metadata_all: v.processing_metadata.as_ref().map(|m| m.ffprobe_metadata_all.clone()).flatten(),
        })
    }
}

// ============================ Comment ============================

impl crate::database::models::Comment
{
    pub fn from_proto3(c: &proto::Comment) -> DBResult<Self>
    {
        //let user = v.user.as_ref().ok_or(anyhow::anyhow!("Missing user"))?;
        let created = c.created.as_ref().ok_or(anyhow::anyhow!("Missing created timestamp"))?;
        Ok(Self {
            id: c.id.parse().map_err(|_| DBError::Other(anyhow::anyhow!("Invalid comment ID")))?,
            media_file_id: c.media_file_id.clone(),
            user_id: c.user_id.clone(),
            username_ifnull: c.username_ifnull.clone(),
            comment: c.comment.clone(),
            timecode: c.timecode.clone(),
            parent_id: c.parent_id.as_ref().map(|id| id.parse()).transpose().map_err(|_| DBError::Other(anyhow::anyhow!("Invalid parent ID")))?,
            created: proto3_to_datetime(created).ok_or(anyhow::anyhow!("Invalid 'created' timestamp"))?,
            edited: c.edited.as_ref().map(|t| proto3_to_datetime(t)).flatten(),
            drawing: c.drawing.clone(),
        })
    }

    pub fn to_proto3(&self) -> proto::Comment
    {
        let created_timestamp = Some(datetime_to_proto3(&self.created));
        let edited_timestamp = self.edited.map(|edited| datetime_to_proto3(&edited));

        proto::Comment {
            id: self.id.to_string(),
            media_file_id: self.media_file_id.clone(),
            user_id: self.user_id.clone(),
            username_ifnull: self.username_ifnull.clone(),
            comment: self.comment.clone(),
            timecode: self.timecode.clone(),
            parent_id: self.parent_id.map(|id| id.to_string()),
            created: created_timestamp,
            edited: edited_timestamp,
            drawing: self.drawing.clone(),
        }
    }
}

impl crate::database::models::CommentInsert
{
    pub fn from_proto3(c: &proto::Comment) -> DBResult<Self>
    {
        if c.id != String::default() {
            return Err(DBError::Other(anyhow::anyhow!("Comment ID must be empty for conversion to CommentInsert, which doesn't have 'id' field")));
        }
        Ok(Self {
            media_file_id: c.media_file_id.clone(),
            user_id: c.user_id.clone(),
            username_ifnull: c.username_ifnull.clone(),
            comment: c.comment.clone(),
            timecode: c.timecode.clone(),
            parent_id: c.parent_id.as_ref().map(|id| id.parse()).transpose().map_err(|_| DBError::Other(anyhow::anyhow!("Invalid parent ID")))?,
            drawing: c.drawing.clone(),
        })
    }
}

// ============================ Message ============================

impl crate::database::models::Message
{
    pub fn from_proto3(v: &proto::UserMessage) -> DBResult<Self>
    {
        let created = v.created.as_ref().ok_or(anyhow::anyhow!("Missing created timestamp"))?;
        let user_id = v.user_id.as_ref().ok_or(anyhow::anyhow!("Missing user"))?;
        let id = v.id.as_ref().ok_or(anyhow::anyhow!("Missing message ID"))?;
        Ok(Self {
            id: id.parse().map_err(|_| DBError::Other(anyhow::anyhow!("Invalid message ID")))?,
            event_name: proto_msg_type_to_event_name(v.r#type()).to_string(),
            user_id: user_id.clone(),
            media_file_id: v.refs.as_ref().map(|r| r.media_file_id.clone()).flatten(),
            comment_id: v.refs.as_ref().map(|r| r.comment_id.as_ref().map(|id| id.parse()).transpose().map_err(|_| DBError::Other(anyhow::anyhow!("Invalid comment ID")))).transpose()?.flatten(),
            message: v.message.clone(),
            details: v.details.clone().unwrap_or_default(),
            created: proto3_to_datetime(created).ok_or(anyhow::anyhow!("Invalid 'created' timestamp"))?,
            seen: v.seen,
        })
    }

    pub fn to_proto3(&self) -> proto::UserMessage
    {
        proto::UserMessage {
            id: Some(self.id.to_string()),
            r#type: msg_event_name_to_proto_msg_type(&self.event_name.as_str()).into(),
            user_id: Some(self.user_id.clone()),
            refs:Some(proto::user_message::Refs {
                media_file_id: self.media_file_id.clone(),
                comment_id: self.comment_id.map(|id| id.to_string()),
            }),
            message: self.message.clone(),
            details: if self.details.is_empty() { None } else { Some(self.details.clone()) },
            created: Some(datetime_to_proto3(&self.created)),
            seen: self.seen
        }
    }
}

impl crate::database::models::MessageInsert
{
    pub fn from_proto3(v: &proto::UserMessage) -> DBResult<Self>
    {
        if v.id.is_some() {
            return Err(DBError::Other(anyhow::anyhow!("Message ID must be empty for conversion to MessageInsert, which doesn't have 'id' field")));
        }
        let user_id = v.user_id.as_ref().ok_or(anyhow::anyhow!("Missing user"))?;

        Ok(Self {
            event_name: proto_msg_type_to_event_name(v.r#type()).to_string(),
            user_id: user_id.clone(),
            media_file_id: v.refs.as_ref().map(|r| r.media_file_id.clone()).flatten(),
            comment_id: v.refs.as_ref().map(|r| r.comment_id.as_ref().map(|id| id.parse()).transpose().map_err(|_| DBError::Other(anyhow::anyhow!("Invalid comment ID")))).transpose()?.flatten(),
            message: v.message.clone(),
            details: v.details.clone().unwrap_or_default(),
            seen: v.seen,
        })
    }

    pub fn to_proto3(&self) -> proto::UserMessage
    {
        proto::UserMessage {
            id: None,
            r#type: msg_event_name_to_proto_msg_type(&self.event_name.as_str()).into(),
            user_id: Some(self.user_id.clone()),
            refs:Some(proto::user_message::Refs {
                media_file_id: self.media_file_id.clone(),
                comment_id: self.comment_id.map(|id| id.to_string()),
            }),
            message: self.message.clone(),
            details: if self.details.is_empty() { None } else { Some(self.details.clone()) },
            created: None,
            seen: self.seen
        }
    }
}
