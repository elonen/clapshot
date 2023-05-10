// @generated automatically by Diesel CLI.

diesel::table! {
    videos (id) {
        id -> Text,
        user_id -> Nullable<Text>,
        user_name -> Nullable<Text>,
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
        video_id -> Text,
        parent_id -> Nullable<Integer>,
        created -> Timestamp,
        edited -> Nullable<Timestamp>,
        user_id -> Text,
        user_name -> Text,
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
        video_id -> Nullable<Text>,
        comment_id -> Nullable<Integer>,
        event_name -> Text,
        message -> Text,
        details -> Text,
    }
}
diesel::joinable!(messages -> comments (comment_id));


diesel::table! {
    prop_edges (id) {
        id -> Integer,
        from_video -> Nullable<Text>,
        from_comment -> Nullable<Integer>,
        from_node -> Nullable<Integer>,
        to_video -> Nullable<Text>,
        to_comment -> Nullable<Integer>,
        to_node -> Nullable<Integer>,
        edge_type -> Text,
        body -> Nullable<Text>,
        sort_order -> Nullable<Float>,
        sibling_id -> Nullable<Integer>,
    }
}

diesel::table! {
    prop_nodes (id) {
        id -> Integer,
        node_type -> Text,
        body -> Nullable<Text>,
        singleton_key -> Nullable<Text>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    comments,
    messages,
    prop_edges,
    prop_nodes,
    videos,
);
