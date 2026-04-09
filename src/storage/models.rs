use diesel::prelude::*;

use super::schema::{sessions, threads, turns, turn_tool_calls};

// ── Session ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = sessions)]
pub struct SessionRow {
    pub id: String,
    pub binding_id: String,
    pub active_thread_id: Option<String>,
    pub auto_approved_tools: String,
    pub metadata: String,
    pub created_at: String,
    pub last_active_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSessionRow<'a> {
    pub id: &'a str,
    pub binding_id: &'a str,
    pub active_thread_id: Option<&'a str>,
    pub auto_approved_tools: &'a str,
    pub metadata: &'a str,
    pub created_at: &'a str,
    pub last_active_at: &'a str,
}

// ── Thread ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = threads)]
pub struct ThreadRow {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub channel: String,
    pub external_thread_id: Option<String>,
    pub state: String,
    pub metadata: String,
    pub pending_approvals: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = threads)]
pub struct NewThreadRow<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub user_id: &'a str,
    pub channel: &'a str,
    pub external_thread_id: Option<&'a str>,
    pub state: &'a str,
    pub metadata: &'a str,
    pub pending_approvals: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

// ── Turn ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = turns)]
pub struct TurnRow {
    pub id: String,
    pub thread_id: String,
    pub session_id: String,
    pub turn_number: i32,
    pub user_input: String,
    pub thinking: Option<String>,
    pub response: Option<String>,
    pub state: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub current_tool_iterations: i32,
    pub draft_message_id: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = turns)]
pub struct NewTurnRow<'a> {
    pub id: &'a str,
    pub thread_id: &'a str,
    pub session_id: &'a str,
    pub turn_number: i32,
    pub user_input: &'a str,
    pub thinking: Option<&'a str>,
    pub response: Option<&'a str>,
    pub state: &'a str,
    pub started_at: &'a str,
    pub completed_at: Option<&'a str>,
    pub error: Option<&'a str>,
    pub current_tool_iterations: i32,
    pub draft_message_id: Option<&'a str>,
}

// ── ToolCall ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = turn_tool_calls)]
pub struct ToolCallRow {
    pub id: i32,
    pub turn_id: String,
    pub call_index: i32,
    pub name: String,
    pub parameters: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = turn_tool_calls)]
pub struct NewToolCallRow<'a> {
    pub turn_id: &'a str,
    pub call_index: i32,
    pub name: &'a str,
    pub parameters: &'a str,
    pub result: Option<&'a str>,
    pub error: Option<&'a str>,
}