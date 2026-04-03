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

    async fn start_typing(&self) -> Result<()> {
        Ok(())
    }

    async fn end_typing(&self) -> Result<()> {
        Ok(())
    }

    async fn reaction(&self, _message_id: &str, _emoji: char) -> Result<()> {
        Ok(())
    }

    /// Send a message (final or draft).
    async fn send(&self, response: OutgoingResponse) -> Result<()>;

    /// Whether this channel supports draft updates (streaming).
    fn supports_draft_updates(&self) -> bool {
        false
    }

    /// Send an initial draft message. Returns a platform-specific message ID for later edits.
    async fn send_draft(&self, _message: &OutgoingResponse) -> Result<Option<String>> {
        Ok(None)
    }

    /// Update a previously sent draft message with new accumulated content.
    ///
    /// Returns `Ok(None)` to keep the current draft message ID, or
    /// `Ok(Some(new_id))` when a continuation message was created
    /// (e.g. after hitting a platform edit-count cap).
    async fn update_draft(&self, _message_id: &str, _text: &str) -> Result<Option<String>> {
        Ok(None)
    }

    /// Finalize a draft with the complete response (e.g. apply Markdown formatting).
    async fn finalize_draft(&self, _message_id: &str, _text: &str) -> Result<()> {
        Ok(())
    }

    /// Cancel and remove a previously sent draft message if the channel supports it.
    async fn cancel_draft(&self, _message_id: &str) -> Result<()> {
        Ok(())
    }

    /// Platform name (e.g., "websocket", "discord").
    fn platform(&self) -> &str;
}
