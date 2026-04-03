use crate::message::ContentPart;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnToolCall {
    /// Tool name.
    pub name: String,
    /// Parameters passed to the tool.
    pub parameters: serde_json::Value,
    /// Result from the tool (if successful).
    pub result: Option<serde_json::Value>,
    /// Error from the tool (if failed).
    pub error: Option<String>,
}

/// State of a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnState {
    /// Turn is being processed.
    Processing,
    /// Turn completed successfully.
    Completed,
    /// Turn failed with an error.
    Failed,
    /// Turn was interrupted.
    Interrupted,
}

/// A single turn (request/response pair) in a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    /// Unique ID.
    pub id: Uuid,
    /// Unique thread ID.
    pub thread_id: Uuid,
    /// Unique session ID.
    pub session_id: Uuid,
    /// Turn number (0-indexed).
    pub turn_number: usize,
    /// User input that started this turn.
    pub user_input: String,
    pub thinking: Option<String>,
    /// Agent response (if completed).
    pub response: Option<String>,
    /// Tool calls made during this turn.
    pub tool_calls: Vec<TurnToolCall>,
    /// Turn state.
    pub state: TurnState,
    /// When the turn started.
    pub started_at: DateTime<Utc>,
    /// When the turn completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Transient image content parts for multimodal LLM input.
    /// Not serialized — images are only needed for the current LLM call.
    /// The text description in `user_input` persists for compaction/context.
    #[serde(skip)]
    pub image_content_parts: Vec<ContentPart>,

    pub current_tool_iterations: usize,

    draft_message_id: Option<String>,
}

impl Turn {
    /// Create a new turn.
    pub fn new(
        session_id: Uuid,
        thread_id: Uuid,
        turn_number: usize,
        user_input: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            session_id,
            turn_number,
            user_input: user_input.into(),
            thinking: None,
            response: None,
            tool_calls: Vec::new(),
            state: TurnState::Processing,
            started_at: Utc::now(),
            completed_at: None,
            error: None,
            image_content_parts: Vec::new(),
            current_tool_iterations: 0,
            draft_message_id: None,
        }
    }

    /// Complete this turn.
    pub fn complete(&mut self, response: impl Into<String>) {
        self.response = Some(response.into());
        self.state = TurnState::Completed;
        self.completed_at = Some(Utc::now());
        // Free image data — only needed for the initial LLM call, not subsequent turns
        self.image_content_parts.clear();
    }

    /// Fail this turn.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.state = TurnState::Failed;
        self.completed_at = Some(Utc::now());
        self.image_content_parts.clear();
    }

    /// Interrupt this turn.
    pub fn interrupt(&mut self) {
        self.state = TurnState::Interrupted;
        self.completed_at = Some(Utc::now());
        self.image_content_parts.clear();
    }

    pub fn record_draft_message_id(&mut self, draft_message_id: Option<String>) {
        self.draft_message_id = draft_message_id;
    }

    /// Record a tool call.
    pub fn record_tool_call(&mut self, name: impl Into<String>, params: serde_json::Value) {
        self.tool_calls.push(TurnToolCall {
            name: name.into(),
            parameters: params,
            result: None,
            error: None,
        });
    }

    /// Record tool call result.
    pub fn record_tool_result(&mut self, result: serde_json::Value) {
        if let Some(call) = self.tool_calls.last_mut() {
            call.result = Some(result);
        }
    }

    /// Record tool call error.
    pub fn record_tool_error(&mut self, error: impl Into<String>) {
        if let Some(call) = self.tool_calls.last_mut() {
            call.error = Some(error.into());
        }
    }
}
