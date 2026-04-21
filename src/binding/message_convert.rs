use crate::attachment::AttachmentManager;
use crate::message::{ChatMessage, MediaKind, MessageAttachment, Role as ChatRole};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rig::OneOrMany;
use rig::completion::message::{
    AssistantContent, Audio, AudioMediaType, Document, DocumentMediaType, DocumentSourceKind,
    Image, ImageMediaType, Message as RigMessage, MimeType, Text, UserContent,
};

async fn attachment_to_user_content(a: &MessageAttachment, mgr: &AttachmentManager) -> Option<UserContent> {
    let bytes = match mgr.get_bytes(&a.attachment_id).await {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to load attachment {}: {}", a.attachment_id, e);
            return None;
        }
    };

    let b64 = STANDARD.encode(&bytes);
    let data = DocumentSourceKind::Base64(b64);

    Some(match a.kind {
        MediaKind::Image => UserContent::Image(Image {
            data,
            media_type: ImageMediaType::from_mime_type(&a.mime_type),
            detail: None,
            additional_params: None,
        }),
        MediaKind::Audio => UserContent::Audio(Audio {
            data,
            media_type: AudioMediaType::from_mime_type(&a.mime_type),
            additional_params: None,
        }),
        MediaKind::Document => UserContent::Document(Document {
            data,
            media_type: DocumentMediaType::from_mime_type(&a.mime_type),
            additional_params: None,
        }),
    })
}

pub async fn to_rig_message(msg: &ChatMessage, mgr: &AttachmentManager) -> anyhow::Result<RigMessage> {
    match msg.role {
        ChatRole::User | ChatRole::System => {
            let mut parts: Vec<UserContent> = Vec::new();
            for a in &msg.attachments {
                if let Some(content) = attachment_to_user_content(a, mgr).await {
                    parts.push(content);
                }
            }
            parts.push(UserContent::Text(Text { text: msg.content.clone() }));
            Ok(RigMessage::User {
                content: OneOrMany::many(parts).unwrap_or_else(|_| OneOrMany::one(UserContent::Text(Text { text: msg.content.clone() }))),
            })
        }
        ChatRole::Assistant | ChatRole::Tool => Ok(RigMessage::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::Text(Text { text: msg.content.clone() })),
        }),
    }
}

pub async fn to_rig_messages(messages: &[ChatMessage], mgr: &AttachmentManager) -> anyhow::Result<Vec<RigMessage>> {
    let mut result = Vec::with_capacity(messages.len());
    for msg in messages {
        result.push(to_rig_message(msg, mgr).await?);
    }
    Ok(result)
}