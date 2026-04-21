use super::attachment::MessageAttachment;
use super::role::Role;
use super::tool::ToolCall;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// File or media attachments on this message (images, audio, documents).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MessageAttachment>,
    /// Tool call ID if this is a tool result message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Name of the tool for tool results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls made by the assistant (OpenAI protocol requires these
    /// to appear on the assistant message preceding tool result messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            attachments: Vec::new(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            attachments: Vec::new(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }

    /// Create a user message with attachments (images, audio, documents).
    pub fn user_with_attachments(content: impl Into<String>, attachments: Vec<MessageAttachment>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            attachments,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            attachments: Vec::new(),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }
    }

    /// Create an assistant message that includes tool calls.
    ///
    /// Per the OpenAI protocol, an assistant message with tool_calls must
    /// precede the corresponding tool result messages in the conversation.
    pub fn assistant_with_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.unwrap_or_default(),
            attachments: Vec::new(),
            tool_call_id: None,
            name: None,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
        }
    }

    /// Create a tool result message.
    pub fn tool_result(tool_call_id: impl Into<String>, name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            attachments: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
            tool_calls: None,
        }
    }
}
