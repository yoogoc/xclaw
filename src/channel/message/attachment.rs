/// Kind of attachment carried on an incoming message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentKind {
    /// Audio content (voice notes, audio files).
    Audio,
    /// Image content (photos, screenshots).
    Image,
    /// Document content (PDFs, files).
    Document,
}

impl AttachmentKind {
    /// Infer attachment kind from MIME type.
    pub fn from_mime_type(mime: &str) -> Self {
        let base = mime.split(';').next().unwrap_or(mime).trim();
        if base.starts_with("audio/") {
            Self::Audio
        } else if base.starts_with("image/") {
            Self::Image
        } else {
            Self::Document
        }
    }
}

/// A file or media attachment on an incoming message.
#[derive(Debug, Clone)]
pub struct IncomingAttachment {
    /// Unique identifier within the channel (e.g., Telegram file_id).
    pub id: String,
    /// What kind of content this is.
    pub kind: AttachmentKind,
    /// MIME type (e.g., "image/jpeg", "audio/ogg", "application/pdf").
    pub mime_type: String,
    /// Original filename, if known.
    pub filename: Option<String>,
    /// File size in bytes, if known.
    pub size_bytes: Option<u64>,
    /// URL to download the file from the channel's API.
    pub source_url: Option<String>,
    /// Opaque key for host-side storage (e.g., after download/caching).
    pub storage_key: Option<String>,
    /// Extracted text content (e.g., OCR result, PDF text, audio transcript).
    pub extracted_text: Option<String>,
    /// Raw file bytes (for small files downloaded by the channel).
    pub data: Vec<u8>,
    /// Duration in seconds (for audio/video).
    pub duration_secs: Option<u32>,
}
