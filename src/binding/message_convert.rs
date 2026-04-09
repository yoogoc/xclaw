use crate::message::{ChatMessage, Role as ChatRole};
use rig::OneOrMany;
use rig::completion::message::{AssistantContent, Message as RigMessage, Text, UserContent};

/// Convert ChatMessage to rig::completion::Message
pub fn to_rig_message(msg: &ChatMessage) -> Result<RigMessage, anyhow::Error> {
    match msg.role {
        ChatRole::User | ChatRole::System => Ok(RigMessage::User {
            content: OneOrMany::many(vec![UserContent::Text(Text { text: msg.content.clone() })])?,
        }),
        ChatRole::Assistant | ChatRole::Tool => Ok(RigMessage::Assistant {
            id: None,
            content: OneOrMany::many(vec![AssistantContent::Text(Text { text: msg.content.clone() })])?,
        }),
    }
}

/// Convert Vec<ChatMessage> to Vec<rig::completion::Message>
pub fn to_rig_messages(messages: &[ChatMessage]) -> Result<Vec<RigMessage>, anyhow::Error> {
    messages.iter().map(to_rig_message).collect()
}
