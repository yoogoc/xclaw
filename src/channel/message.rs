mod attachment;

use crate::channel::message::attachment::IncomingAttachment;
use chrono::{DateTime, Utc};
use futures::Stream;
use std::pin::Pin;
use uuid::Uuid;

/// A message received from an external channel.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// Unique message ID.
    pub id: Uuid,
    /// Channel this message came from.
    pub channel: String,
    /// User identifier within the channel.
    pub user_id: String,
    /// Optional display name.
    pub user_name: Option<String>,
    /// Message content.
    pub content: String,
    /// Thread/conversation ID for threaded conversations.
    pub thread_id: Option<String>,
    /// When the message was received.
    pub received_at: DateTime<Utc>,
    /// Channel-specific metadata.
    pub metadata: serde_json::Value,
    /// IANA timezone string from the client (e.g. "America/New_York").
    pub timezone: Option<String>,
    /// File or media attachments on this message.
    pub attachments: Vec<IncomingAttachment>,
}

pub type MessageStream = Pin<Box<dyn Stream<Item = IncomingMessage> + Send>>;

/// Response to send back to a channel.
#[derive(Debug, Clone)]
pub struct OutgoingResponse {
    /// The content to send.
    pub content: String,
    /// Optional thread ID to reply in.
    pub thread_id: Option<String>,
    /// Whether this is a draft (streaming) or final message.
    pub is_draft: bool,
    /// Optional file paths to attach.
    pub attachments: Vec<String>,
    /// Channel-specific metadata for the response.
    pub metadata: serde_json::Value,
}

/// Message to send through a channel
#[allow(unused)]
#[derive(Debug, Clone)]
pub struct SendMessage {
    pub content: String,
    pub recipient: String,
    pub subject: Option<String>,
    /// Platform thread identifier for threaded replies (e.g. Slack `thread_ts`).
    pub thread_ts: Option<String>,
}
