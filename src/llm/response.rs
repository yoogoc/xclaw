use crate::message::ToolCall;
use rig::OneOrMany;
use rig::completion::AssistantContent;

/// Why the completion finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
    ToolUse,
    Reasoning,
    ContentFilter,
    Unknown,
}

pub struct LLMResponse {
    pub thinking: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Tokens read from the provider's server-side prompt cache (Anthropic).
    pub cache_read_input_tokens: u32,
    /// Tokens written to the provider's server-side prompt cache (Anthropic).
    pub cache_creation_input_tokens: u32,
}

impl LLMResponse {
    pub fn new(choice: OneOrMany<AssistantContent>) -> Self {
        let mut content_parts: Vec<String> = Vec::new();
        let mut thinking_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for content in choice.iter() {
            match content {
                AssistantContent::Text(t) => {
                    if !t.text.is_empty() {
                        content_parts.push(t.text.clone());
                    }
                }
                AssistantContent::ToolCall(tc) => {
                    tool_calls.push(ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: tc.function.arguments.clone(),
                    });
                }
                AssistantContent::Reasoning(r) => thinking_parts.push(r.display_text()),
                _ => {}
            }
        }

        let content = if content_parts.is_empty() { None } else { Some(content_parts.join("")) };

        let thinking = if thinking_parts.is_empty() { None } else { Some(thinking_parts.join("")) };

        let finish_reason = if tool_calls.is_empty() {
            // FinishReason::Stop
            if content.is_none() {
                if thinking.is_none() {
                    // no content, no reasoning, no tool call!!!
                    FinishReason::Stop
                } else {
                    FinishReason::Reasoning
                }
            } else {
                FinishReason::Stop
            }
        } else {
            FinishReason::ToolUse
        };

        Self {
            thinking,
            content,
            tool_calls,
            input_tokens: 0,
            output_tokens: 0,
            finish_reason,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
        }
    }
}

impl From<OneOrMany<AssistantContent>> for LLMResponse {
    fn from(ac: OneOrMany<AssistantContent>) -> Self {
        Self::new(ac)
    }
}
