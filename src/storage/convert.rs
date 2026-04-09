use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::session::{PendingApproval, Session, Thread, ThreadState, Turn, TurnState, TurnToolCall};

use super::models::*;

// ── ThreadState <-> &str ──

pub fn thread_state_to_str(state: ThreadState) -> &'static str {
    match state {
        ThreadState::Idle => "Idle",
        ThreadState::Processing => "Processing",
        ThreadState::AwaitingApproval => "AwaitingApproval",
        ThreadState::Completed => "Completed",
        ThreadState::Interrupted => "Interrupted",
    }
}

pub fn thread_state_from_str(s: &str) -> Result<ThreadState> {
    match s {
        "Idle" => Ok(ThreadState::Idle),
        "Processing" => Ok(ThreadState::Processing),
        "AwaitingApproval" => Ok(ThreadState::AwaitingApproval),
        "Completed" => Ok(ThreadState::Completed),
        "Interrupted" => Ok(ThreadState::Interrupted),
        _ => anyhow::bail!("Unknown ThreadState: {}", s),
    }
}

// ── TurnState <-> &str ──

pub fn turn_state_to_str(state: TurnState) -> &'static str {
    match state {
        TurnState::Processing => "Processing",
        TurnState::Completed => "Completed",
        TurnState::Failed => "Failed",
        TurnState::Interrupted => "Interrupted",
    }
}

pub fn turn_state_from_str(s: &str) -> Result<TurnState> {
    match s {
        "Processing" => Ok(TurnState::Processing),
        "Completed" => Ok(TurnState::Completed),
        "Failed" => Ok(TurnState::Failed),
        "Interrupted" => Ok(TurnState::Interrupted),
        _ => anyhow::bail!("Unknown TurnState: {}", s),
    }
}

// ── Timestamp helpers ──

fn dt_to_str(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn str_to_dt(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)).with_context(|| format!("Invalid datetime: {}", s))
}

fn opt_dt_to_str(dt: &Option<DateTime<Utc>>) -> Option<String> {
    dt.as_ref().map(dt_to_str)
}

fn opt_str_to_dt(s: &Option<String>) -> Result<Option<DateTime<Utc>>> {
    match s {
        Some(s) => Ok(Some(str_to_dt(s)?)),
        None => Ok(None),
    }
}

// ── Session conversions ──

pub struct SessionInsertValues {
    pub id: String,
    pub binding_id: String,
    pub active_thread_id: Option<String>,
    pub auto_approved_tools: String,
    pub metadata: String,
    pub created_at: String,
    pub last_active_at: String,
}

impl SessionInsertValues {
    pub fn from_session(session: &Session) -> Self {
        Self {
            id: session.id.to_string(),
            binding_id: session.binding_id.clone(),
            active_thread_id: session.active_thread.map(|id| id.to_string()),
            auto_approved_tools: serde_json::to_string(&session.auto_approved_tools).unwrap_or_else(|_| "[]".to_string()),
            metadata: serde_json::to_string(&session.metadata).unwrap_or_else(|_| "null".to_string()),
            created_at: dt_to_str(&session.created_at),
            last_active_at: dt_to_str(&session.last_active_at),
        }
    }

    pub fn as_new_row(&self) -> NewSessionRow<'_> {
        NewSessionRow {
            id: &self.id,
            binding_id: &self.binding_id,
            active_thread_id: self.active_thread_id.as_deref(),
            auto_approved_tools: &self.auto_approved_tools,
            metadata: &self.metadata,
            created_at: &self.created_at,
            last_active_at: &self.last_active_at,
        }
    }
}

pub fn session_from_row(row: SessionRow, threads: HashMap<Uuid, Thread>) -> Result<Session> {
    Ok(Session {
        id: Uuid::parse_str(&row.id)?,
        binding_id: row.binding_id,
        active_thread: row.active_thread_id.as_deref().map(Uuid::parse_str).transpose()?,
        threads,
        created_at: str_to_dt(&row.created_at)?,
        last_active_at: str_to_dt(&row.last_active_at)?,
        metadata: serde_json::from_str(&row.metadata).unwrap_or(serde_json::Value::Null),
        auto_approved_tools: serde_json::from_str(&row.auto_approved_tools).unwrap_or_default(),
    })
}

// ── Thread conversions ──

pub struct ThreadInsertValues {
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

impl ThreadInsertValues {
    pub fn from_thread(thread: &Thread) -> Self {
        Self {
            id: thread.id.to_string(),
            session_id: thread.session_id.to_string(),
            user_id: thread.user_id.clone(),
            channel: thread.channel.clone(),
            external_thread_id: thread.external_thread_id.clone(),
            state: thread_state_to_str(thread.state).to_string(),
            metadata: serde_json::to_string(&thread.metadata).unwrap_or_else(|_| "null".to_string()),
            pending_approvals: serde_json::to_string(&thread.pending_approvals).unwrap_or_else(|_| "[]".to_string()),
            created_at: dt_to_str(&thread.created_at),
            updated_at: dt_to_str(&thread.updated_at),
        }
    }

    pub fn as_new_row(&self) -> NewThreadRow<'_> {
        NewThreadRow {
            id: &self.id,
            session_id: &self.session_id,
            user_id: &self.user_id,
            channel: &self.channel,
            external_thread_id: self.external_thread_id.as_deref(),
            state: &self.state,
            metadata: &self.metadata,
            pending_approvals: &self.pending_approvals,
            created_at: &self.created_at,
            updated_at: &self.updated_at,
        }
    }
}

pub fn thread_from_row(row: ThreadRow, turns: Vec<Turn>) -> Result<Thread> {
    let pending_approvals: Vec<PendingApproval> = serde_json::from_str(&row.pending_approvals).unwrap_or_default();

    Ok(Thread {
        id: Uuid::parse_str(&row.id)?,
        session_id: Uuid::parse_str(&row.session_id)?,
        user_id: row.user_id,
        channel: row.channel,
        external_thread_id: row.external_thread_id,
        state: thread_state_from_str(&row.state)?,
        turns,
        created_at: str_to_dt(&row.created_at)?,
        updated_at: str_to_dt(&row.updated_at)?,
        metadata: serde_json::from_str(&row.metadata).unwrap_or(serde_json::Value::Null),
        pending_approvals,
    })
}

// ── Turn conversions ──

pub struct TurnInsertValues {
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

impl TurnInsertValues {
    pub fn from_turn(turn: &Turn) -> Self {
        Self {
            id: turn.id.to_string(),
            thread_id: turn.thread_id.to_string(),
            session_id: turn.session_id.to_string(),
            turn_number: turn.turn_number as i32,
            user_input: turn.user_input.clone(),
            thinking: turn.thinking.clone(),
            response: turn.response.clone(),
            state: turn_state_to_str(turn.state).to_string(),
            started_at: dt_to_str(&turn.started_at),
            completed_at: opt_dt_to_str(&turn.completed_at),
            error: turn.error.clone(),
            current_tool_iterations: turn.current_tool_iterations as i32,
            draft_message_id: turn.draft_message_id.clone(),
        }
    }

    pub fn as_new_row(&self) -> NewTurnRow<'_> {
        NewTurnRow {
            id: &self.id,
            thread_id: &self.thread_id,
            session_id: &self.session_id,
            turn_number: self.turn_number,
            user_input: &self.user_input,
            thinking: self.thinking.as_deref(),
            response: self.response.as_deref(),
            state: &self.state,
            started_at: &self.started_at,
            completed_at: self.completed_at.as_deref(),
            error: self.error.as_deref(),
            current_tool_iterations: self.current_tool_iterations,
            draft_message_id: self.draft_message_id.as_deref(),
        }
    }
}

pub fn turn_from_row(row: TurnRow, tool_calls: Vec<TurnToolCall>) -> Result<Turn> {
    Ok(Turn {
        id: Uuid::parse_str(&row.id)?,
        thread_id: Uuid::parse_str(&row.thread_id)?,
        session_id: Uuid::parse_str(&row.session_id)?,
        turn_number: row.turn_number as usize,
        user_input: row.user_input,
        thinking: row.thinking,
        response: row.response,
        tool_calls,
        state: turn_state_from_str(&row.state)?,
        started_at: str_to_dt(&row.started_at)?,
        completed_at: opt_str_to_dt(&row.completed_at)?,
        error: row.error,
        image_content_parts: Vec::new(),
        current_tool_iterations: row.current_tool_iterations as usize,
        draft_message_id: row.draft_message_id,
    })
}

// ── ToolCall conversions ──

pub struct ToolCallInsertValues {
    pub turn_id: String,
    pub call_index: i32,
    pub name: String,
    pub parameters: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl ToolCallInsertValues {
    pub fn from_tool_call(turn_id: &str, index: usize, tc: &TurnToolCall) -> Self {
        Self {
            turn_id: turn_id.to_string(),
            call_index: index as i32,
            name: tc.name.clone(),
            parameters: serde_json::to_string(&tc.parameters).unwrap_or_else(|_| "{}".to_string()),
            result: tc.result.as_ref().map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".to_string())),
            error: tc.error.clone(),
        }
    }

    pub fn as_new_row(&self) -> NewToolCallRow<'_> {
        NewToolCallRow {
            turn_id: &self.turn_id,
            call_index: self.call_index,
            name: &self.name,
            parameters: &self.parameters,
            result: self.result.as_deref(),
            error: self.error.as_deref(),
        }
    }
}

pub fn tool_call_from_row(row: ToolCallRow) -> TurnToolCall {
    TurnToolCall {
        name: row.name,
        parameters: row.parameters.parse::<serde_json::Value>().unwrap_or(serde_json::Value::Object(Default::default())),
        result: row.result.and_then(|s| serde_json::from_str(&s).ok()),
        error: row.error,
    }
}
