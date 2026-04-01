mod channel;
mod message;
mod websocket;
mod discord;

pub use channel::*;
pub use message::{IncomingMessage, MessageStream, OutgoingResponse};
pub use websocket::WebSocketChannel;
pub use discord::{DiscordChannel, DiscordConfig};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ChannelManager {
    channel: Arc<dyn Channel>,
    draft_buffer: Arc<RwLock<HashMap<String, String>>>,
}

impl ChannelManager {
    pub fn new(channel: impl Channel + 'static) -> Self {
        Self {
            channel: Arc::new(channel),
            draft_buffer: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.channel.start().await
    }

    pub async fn receive(&self) -> anyhow::Result<MessageStream> {
        self.channel.receive().await
    }

    pub async fn send_chunk(&self, thread_id: &str, chunk: &str) -> anyhow::Result<()> {
        if self.channel.supports_draft_updates() {
            let mut buffer = self.draft_buffer.write().await;
            let content = buffer.entry(thread_id.to_string()).or_insert_with(String::new);
            content.push_str(chunk);

            self.channel.send(OutgoingResponse {
                content: content.clone(),
                thread_id: Some(thread_id.to_string()),
                is_draft: true,
                attachments: vec![],
                metadata: serde_json::Value::Null,
            }).await?;
        } else {
            let mut buffer = self.draft_buffer.write().await;
            buffer.entry(thread_id.to_string()).or_insert_with(String::new).push_str(chunk);
        }
        Ok(())
    }

    pub async fn send_final(&self, thread_id: &str) -> anyhow::Result<()> {
        let content = {
            let mut buffer = self.draft_buffer.write().await;
            buffer.remove(thread_id).unwrap_or_default()
        };

        self.channel.send(OutgoingResponse {
            content,
            thread_id: Some(thread_id.to_string()),
            is_draft: false,
            attachments: vec![],
            metadata: serde_json::Value::Null,
        }).await
    }
}
