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
        thumb_sheet_cols -> Nullable<Integer>,
        thumb_sheet_rows -> Nullable<Integer>,
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
        media_file_id -> Text,
        parent_id -> Nullable<Integer>,
        created -> Timestamp,
        edited -> Nullable<Timestamp>,
        user_id -> Nullable<Text>,
        username_ifnull -> Text,
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
        media_file_id -> Nullable<Text>,
        comment_id -> Nullable<Integer>,
        event_name -> Text,
        message -> Text,
        details -> Text,
    }
}
diesel::joinable!(messages -> comments (comment_id));


diesel::allow_tables_to_appear_in_same_query!(
    users,
    comments,
    messages,
    media_files,
    media_types,
);
