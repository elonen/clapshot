// @generated automatically by Diesel CLI.

diesel::table! {
    users (id) {
        id -> Text,
        name -> Text,
        created -> Timestamp,
    }
}

diesel::table! {
    media_types (id) {
        id -> Text,
    }
}

diesel::table! {
    media_files (id) {
        id -> Text,
        user_id -> Text,
        media_type -> Nullable<Text>,
        added_time -> Timestamp,
        recompression_done -> Nullable<Timestamp>,
        thumbs_done -> Nullable<Timestamp>,
        has_thumbnail -> Nullable<Bool>,
        thumb_sheet_cols -> Nullable<Integer>,
        thumb_sheet_rows -> Nullable<Integer>,
        orig_filename -> Nullable<Text>,
        title -> Nullable<Text>,
        total_frames -> Nullable<Integer>,
        duration -> Nullable<Float>,
        fps -> Nullable<Text>,
        raw_metadata_all -> Nullable<Text>,
        default_subtitle_id -> Nullable<Integer>,
    }
}

diesel::table! {
    comments (id) {
        id -> Integer,
        media_file_id -> Text,
        parent_id -> Nullable<Integer>,
        created -> Timestamp,
        edited -> Nullable<Timestamp>,
        user_id -> Nullable<Text>,
        username_ifnull -> Text,
        comment -> Text,
        timecode -> Nullable<Text>,
        drawing -> Nullable<Text>,
        subtitle_id -> Nullable<Integer>,
        subtitle_filename_ifnull -> Nullable<Text>,
    }
}

diesel::table! {
    messages (id) {
        id -> Integer,
        user_id -> Text,
        created -> Timestamp,
        seen -> Bool,
        media_file_id -> Nullable<Text>,
        comment_id -> Nullable<Integer>,
        subtitle_id -> Nullable<Integer>,
        event_name -> Text,
        message -> Text,
        details -> Text,
    }
}
diesel::joinable!(messages -> comments (comment_id));
diesel::joinable!(messages -> media_files (media_file_id));
diesel::joinable!(messages -> subtitles (subtitle_id));

diesel::table! {
    subtitles (id) {
        id -> Integer,
        media_file_id -> Text,
        title -> Text,
        language_code -> Text,
        filename -> Nullable<Text>,
        orig_filename -> Text,
        added_time -> Timestamp,
        time_offset -> Float,
    }
}
diesel::joinable!(comments -> subtitles (subtitle_id));


diesel::allow_tables_to_appear_in_same_query!(
    users,
    comments,
    messages,
    media_files,
    media_types,
    subtitles,
);
