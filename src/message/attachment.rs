use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Audio,
    Image,
    Document,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttachment {
    pub attachment_id: String,
    pub kind: MediaKind,
    pub mime_type: String,
    pub filename: Option<String>,
}