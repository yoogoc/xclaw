// @generated automatically by Diesel CLI.

diesel::table! {
    attachments (id) {
        id -> Text,
        kind -> Text,
        mime_type -> Text,
        filename -> Nullable<Text>,
        size_bytes -> Nullable<Integer>,
        source_url -> Nullable<Text>,
        created_at -> Text,
    }
}

diesel::table! {
    sessions (id) {
        id -> Text,
        binding_id -> Text,
        active_thread_id -> Nullable<Text>,
        auto_approved_tools -> Text,
        metadata -> Text,
        created_at -> Text,
        last_active_at -> Text,
    }
}

diesel::table! {
    threads (id) {
        id -> Text,
        session_id -> Text,
        user_id -> Text,
        channel -> Text,
        external_thread_id -> Nullable<Text>,
        state -> Text,
        metadata -> Text,
        pending_approvals -> Text,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    turn_tool_calls (id) {
        id -> Nullable<Integer>,
        turn_id -> Text,
        call_index -> Integer,
        name -> Text,
        parameters -> Text,
        result -> Nullable<Text>,
        error -> Nullable<Text>,
    }
}

diesel::table! {
    turns (id) {
        id -> Text,
        thread_id -> Text,
        session_id -> Text,
        turn_number -> Integer,
        user_input -> Text,
        thinking -> Nullable<Text>,
        response -> Nullable<Text>,
        state -> Text,
        started_at -> Text,
        completed_at -> Nullable<Text>,
        error -> Nullable<Text>,
        current_tool_iterations -> Integer,
        draft_message_id -> Nullable<Text>,
        attachments -> Text,
    }
}

diesel::joinable!(threads -> sessions (session_id));
diesel::joinable!(turns -> threads (thread_id));
diesel::joinable!(turn_tool_calls -> turns (turn_id));

diesel::allow_tables_to_appear_in_same_query!(attachments, sessions, threads, turn_tool_calls, turns,);