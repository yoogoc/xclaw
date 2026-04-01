use crate::channel::{MessageStream, OutgoingResponse};
use anyhow::Result;

#[async_trait]
pub trait Channel: Send + Sync {
    /// Start the channel (connect, authenticate, etc.).
    async fn start(&self) -> Result<()> {
        Ok(())
    }

    /// Receive incoming messages as a stream.
    async fn receive(&self) -> Result<MessageStream>;

    /// Send a message (final or draft).
    async fn send(&self, response: OutgoingResponse) -> Result<()>;

    /// Whether this channel supports draft updates (streaming).
    fn supports_draft_updates(&self) -> bool {
        false
    }

    /// Platform name (e.g., "websocket", "discord").
    fn platform(&self) -> &str;
}
