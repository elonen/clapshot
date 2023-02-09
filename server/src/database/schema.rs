// @generated automatically by Diesel CLI.

diesel::table! {
    videos (id) {
        id -> Integer,
        video_hash -> Text,
        added_by_userid -> Nullable<Text>,
        added_by_username -> Nullable<Text>,
        added_time -> Timestamp,
        recompression_done -> Nullable<Timestamp>,
        thumb_sheet_dims -> Nullable<Text>,  // "10x10"
        orig_filename -> Nullable<Text>,
        title -> Nullable<Text>,
        total_frames -> Nullable<Integer>,
        duration -> Nullable<Float>,
        fps -> Nullable<Text>,
        raw_metadata_all -> Nullable<Text>,
    }
}

diesel::table! {
    comments (id) {
        id -> Integer,
        video_hash -> Text,
        parent_id -> Nullable<Integer>,
        created -> Timestamp,
        edited -> Nullable<Timestamp>,
        user_id -> Text,
        username -> Text,
        comment -> Text,
        timecode -> Nullable<Text>,
        drawing -> Nullable<Text>,
    }
}

diesel::table! {
    messages (id) {
        id -> Integer,
        user_id -> Text,
        created -> Timestamp,
        seen -> Bool,
        ref_video_hash -> Nullable<Text>,
        ref_comment_id -> Nullable<Integer>,
        event_name -> Text,
        message -> Text,
        details -> Text,
    }
}

diesel::joinable!(messages -> comments (ref_comment_id));

diesel::allow_tables_to_appear_in_same_query!(
    comments,
    messages,
    videos,
);
