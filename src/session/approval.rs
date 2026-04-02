use crate::message::ChatMessage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Pending tool approval request stored on a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub auto_approved: bool,
    /// Unique request ID.
    pub request_id: Uuid,
    /// Tool name requiring approval.
    pub tool_name: String,
    /// Tool parameters (original values, used for execution).
    pub parameters: serde_json::Value,
    /// Redacted tool parameters (sensitive values replaced with `[REDACTED]`).
    /// Used for display in approval UI, logs, and SSE broadcasts.
    #[serde(default)]
    pub display_parameters: serde_json::Value,
    /// Description of what the tool will do.
    pub description: String,
    /// Tool call ID from LLM (for proper context continuation).
    pub tool_call_id: String,
    /// Context messages at the time of the request (to resume from).
    pub context_messages: Vec<ChatMessage>,
    /// User timezone at the time the approval was requested, so it persists
    /// through the approval flow even if the approval message lacks timezone.
    #[serde(default)]
    pub user_timezone: Option<String>,
}
