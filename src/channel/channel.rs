use crate::channel::message::SendMessage;
use crate::channel::{IncomingMessage, MessageStream, OutgoingResponse};

#[async_trait]
pub trait Channel: Send + Sync {
    /// Human-readable channel name
    fn name(&self) -> &str;

    /// Check if channel is healthy
    async fn health_check(&self) -> bool {
        true
    }

    async fn receive(&self) -> anyhow::Result<MessageStream> {
        todo!()
    }

    async fn send(
        &self,
        msg: &IncomingMessage,
        response: OutgoingResponse,
    ) -> anyhow::Result<MessageStream> {
        todo!()
    }

    /// Add a reaction (emoji) to a message.
    ///
    /// `channel_id` is the platform channel/conversation identifier (e.g. Discord channel ID).
    /// `message_id` is the platform-scoped message identifier (e.g. `discord_<snowflake>`).
    /// `emoji` is the Unicode emoji to react with (e.g. "👀", "✅").
    async fn add_reaction(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _emoji: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Whether this channel supports progressive message updates via draft edits.
    fn supports_draft_updates(&self) -> bool {
        false
    }

    /// Send an initial draft message. Returns a platform-specific message ID for later edits.
    async fn send_draft(&self, _message: &SendMessage) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// Update a previously sent draft message with new accumulated content.
    ///
    /// Returns `Ok(None)` to keep the current draft message ID, or
    /// `Ok(Some(new_id))` when a continuation message was created
    /// (e.g. after hitting a platform edit-count cap).
    async fn update_draft(
        &self,
        _recipient: &str,
        _message_id: &str,
        _text: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// Finalize a draft with the complete response (e.g. apply Markdown formatting).
    async fn finalize_draft(
        &self,
        _recipient: &str,
        _message_id: &str,
        _text: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
