// @generated automatically by Diesel CLI.

diesel::table! {
    videos (id) {
        id -> Integer,
        video_hash -> Text,
        added_by_userid -> Nullable<Text>,
        added_by_username -> Nullable<Text>,
        added_time -> Timestamp,
        recompression_done -> Nullable<Timestamp>,
        orig_filename -> Nullable<Text>,
        total_frames -> Nullable<Integer>,
        duration -> Nullable<Float>,
        fps -> Nullable<Text>,
        raw_metadata_all -> Nullable<Text>,
        title -> Nullable<Text>,
        thumb_sheet_dims -> Nullable<Text>,
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

diesel::table! {
    prop_edges (id) {
        id -> Integer,
        from_video -> Nullable<Text>,
        from_comment -> Nullable<Text>,
        from_node -> Nullable<Integer>,
        to_video -> Nullable<Text>,
        to_comment -> Nullable<Text>,
        to_node -> Nullable<Integer>,
        edge_type -> Text,
        body -> Nullable<Text>,
        sort_order -> Float,
        sibling_id -> Integer,
    }
}

diesel::table! {
    prop_nodes (id) {
        id -> Integer,
        node_type -> Text,
        body -> Nullable<Text>,
    }
}

diesel::joinable!(messages -> comments (ref_comment_id));

diesel::allow_tables_to_appear_in_same_query!(
    comments,
    messages,
    prop_edges,
    prop_nodes,
    videos,
);

// ------------ manually added views ------------

diesel::table! {
    view_videos_pointing_to_node (node_id, video_hash, edge_type, edge_sibling_id) {
        node_id -> Integer,
        node_type -> Text,
        node_body -> Nullable<Text>,

        edge_type -> Text,
        edge_body -> Nullable<Text>,
        edge_sort_order -> Float,
        edge_sibling_id -> Integer,

        video_hash -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
    }
}

diesel::table! {
    view_nodes_pointing_to_video (video_hash, node_id, edge_type, edge_sibling_id) {
        video_hash -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,

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
    view_videos_without_outgoing_edges (video_hash) {
        video_hash -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
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
    view_video_count_outgoing_edges (video_hash, edge_type) {
        video_hash -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
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
    view_video_count_incoming_edges (video_hash, edge_type) {
        video_hash -> Text,
        video_title -> Nullable<Text>,
        video_duration -> Nullable<Float>,
        edge_type -> Text,
        edge_count -> Integer,
    }
}
