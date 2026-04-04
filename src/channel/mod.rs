mod channel;
mod discord;
mod message;
mod websocket;

pub use channel::*;
pub use discord::{DiscordChannel, DiscordConfig};
pub use message::{IncomingMessage, MessageStream, OutgoingResponse};
pub use websocket::WebSocketChannel;

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

    pub async fn start_typing(&self) -> anyhow::Result<()> {
        self.channel.start_typing().await
    }

    pub async fn send_read(&self, message_id: &str) -> anyhow::Result<()> {
        self.channel.reaction(message_id, '👀').await
    }

    pub async fn send(&self, thread_id: &str, content: &str) -> anyhow::Result<()> {
        self.channel
            .send(OutgoingResponse {
                content: content.to_string(),
                thread_id: Some(thread_id.to_string()),
                is_draft: false,
                attachments: vec![],
                metadata: serde_json::Value::Null,
            })
            .await
    }

    pub async fn start_chunk(&self, thread_id: &str) -> anyhow::Result<Option<String>> {
        if self.channel.supports_draft_updates() {
            return self
                .channel
                .send_draft(&OutgoingResponse {
                    content: "...".to_string(),
                    thread_id: Some(thread_id.to_string()),
                    is_draft: true,
                    attachments: vec![],
                    metadata: serde_json::Value::Null,
                })
                .await;
        }
        Ok(None)
    }

    pub async fn send_chunk(
        &self,
        thread_id: &str,
        draft_message_id: Option<String>,
        chunk: &str,
    ) -> anyhow::Result<()> {
        let mut buffer = self.draft_buffer.write().await;
        buffer
            .entry(thread_id.to_string())
            .or_insert_with(String::new)
            .push_str(chunk);
        if self.channel.supports_draft_updates() {
            let mut buffer = self.draft_buffer.write().await;
            let content = buffer
                .entry(thread_id.to_string())
                .or_insert_with(String::new);
            content.push_str(chunk);

            if let Some(message_id) = draft_message_id {
                self.channel.update_draft(&message_id, &content).await?;
            }
        }
        Ok(())
    }

    pub async fn send_final(
        &self,
        thread_id: &str,
        draft_message_id: Option<String>,
    ) -> anyhow::Result<()> {
        let result = {
            let mut content = {
                let mut buffer = self.draft_buffer.write().await;
                buffer.remove(thread_id).unwrap_or_default()
            };

            if content.is_empty() {
                content = "No Response".to_string();
            }

            if self.channel.supports_draft_updates() {
                if let Some(message_id) = draft_message_id {
                    self.channel.finalize_draft(&message_id, &content).await
                } else {
                    Ok(())
                }
            } else {
                self.channel
                    .send(OutgoingResponse {
                        content,
                        thread_id: Some(thread_id.to_string()),
                        is_draft: false,
                        attachments: vec![],
                        metadata: serde_json::Value::Null,
                    })
                    .await
            }
        };

        self.channel.end_typing().await?;
        result
    }
}
