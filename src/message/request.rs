use crate::message::message::ChatMessage;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<ChatMessage>,
    /// Optional per-request model override.
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
    /// Opaque metadata passed through to the provider (e.g. thread_id for chaining).
    pub metadata: std::collections::HashMap<String, String>,
}
