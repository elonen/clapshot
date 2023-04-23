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
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    comments,
    messages,
    prop_edges,
    prop_nodes,
    videos,
);

// ------------ manually added views ------------

diesel::table! {
    view_videos_pointing_to_node (node_id, video_id, edge_type, edge_sibling_id) {
        node_id -> Integer,
        node_type -> Text,
        node_body -> Nullable<Text>,

        edge_type -> Text,
        edge_body -> Nullable<Text>,
        edge_sort_order -> Float,
        edge_sibling_id -> Integer,

        video_id -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
        video_owner -> Nullable<Text>,
    }
}

diesel::table! {
    view_nodes_pointing_to_video (video_id, node_id, edge_type, edge_sibling_id) {
        video_id -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
        video_owner -> Nullable<Text>,

        edge_type -> Text,
        edge_body -> Nullable<Text>,
        edge_sort_order -> Float,
        edge_sibling_id -> Integer,

        node_id -> Integer,
        node_type -> Text,
        node_body -> Nullable<Text>,
    }
}

diesel::table! {
    view_nodes_pointing_to_node (to_node_id, from_node_id, edge_type, edge_sibling_id) {
        to_node_id -> Integer,
        to_node_type -> Text,
        to_node_body -> Nullable<Text>,

        edge_type -> Text,
        edge_body -> Nullable<Text>,
        edge_sort_order -> Float,
        edge_sibling_id -> Integer,

        from_node_id -> Integer,
        from_node_type -> Text,
        from_node_body -> Nullable<Text>,
    }
}

diesel::table! {
    view_nodes_without_outgoing_edges (id) {
        id -> Integer,
        node_type -> Text,
        node_body -> Nullable<Text>,
    }
}

diesel::table! {
    view_videos_without_outgoing_edges (video_id) {
        video_id -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
        video_owner -> Nullable<Text>,
    }
}

diesel::table! {
    view_node_count_outgoing_edges (node_id, edge_type) {
        node_id -> Integer,
        node_body -> Nullable<Text>,
        node_type -> Text,
        edge_type -> Text,
        edge_count -> Integer,
    }
}

diesel::table! {
    view_video_count_outgoing_edges (video_id, edge_type) {
        video_id -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
        video_owner -> Nullable<Text>,

        edge_type -> Text,
        edge_count -> Integer,
    }
}

diesel::table! {
    view_node_count_incoming_edges (node_id, edge_type) {
        node_id -> Integer,
        node_body -> Nullable<Text>,
        node_type -> Text,
        edge_type -> Text,
        edge_count -> Integer,
    }
}

diesel::table! {
    view_video_count_incoming_edges (video_id, edge_type) {
        video_id -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
        video_owner -> Nullable<Text>,

        edge_type -> Text,
        edge_count -> Integer,
    }
}
