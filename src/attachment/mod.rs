use crate::channel::IncomingAttachment;
use crate::message::{MediaKind, MessageAttachment};
use crate::storage::Database;
use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct AttachmentRecord {
    pub id: String,
    pub kind: String,
    pub mime_type: String,
    pub filename: Option<String>,
    pub size_bytes: u64,
}

pub struct AttachmentManager {
    storage_root: PathBuf,
    db: Arc<Database>,
    http_client: reqwest::Client,
}

impl AttachmentManager {
    pub fn new(storage_root: PathBuf, db: Arc<Database>) -> Result<Self> {
        std::fs::create_dir_all(&storage_root)
            .with_context(|| format!("Failed to create attachment storage dir: {}", storage_root.display()))?;

        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { storage_root, db, http_client })
    }

    pub async fn create(&self, incoming: &IncomingAttachment) -> Result<AttachmentRecord> {
        let bytes = if !incoming.data.is_empty() {
            incoming.data.clone()
        } else if let Some(url) = &incoming.source_url {
            let resp = self.http_client.get(url).send().await
                .with_context(|| format!("Failed to download attachment from {}", url))?;
            resp.error_for_status_ref()
                .with_context(|| format!("HTTP error downloading attachment from {}", url))?;
            resp.bytes().await
                .with_context(|| format!("Failed to read attachment bytes from {}", url))?
                .to_vec()
        } else {
            anyhow::bail!("Attachment has no data and no source_url");
        };

        let id = uuid::Uuid::new_v4().to_string();
        let ext = mime_to_ext(&incoming.mime_type);
        let key = format!("{}.{}", id, ext);
        let path = self.storage_root.join(&key);

        tokio::fs::write(&path, &bytes).await
            .with_context(|| format!("Failed to write attachment to {}", path.display()))?;

        let kind = kind_to_str(&incoming.kind);
        let size_bytes = bytes.len() as u64;
        let created_at = chrono::Utc::now().to_rfc3339();

        self.db.insert_attachment(
            id.clone(),
            kind.to_string(),
            incoming.mime_type.clone(),
            incoming.filename.clone(),
            if size_bytes <= i32::MAX as u64 { Some(size_bytes as i32) } else { None },
            incoming.source_url.clone(),
            created_at,
        ).await?;

        Ok(AttachmentRecord {
            id,
            kind: kind.to_string(),
            mime_type: incoming.mime_type.clone(),
            filename: incoming.filename.clone(),
            size_bytes,
        })
    }

    pub async fn ingest(&self, incoming: &[IncomingAttachment]) -> Vec<MessageAttachment> {
        let mut result = Vec::new();
        for a in incoming {
            match self.create(a).await {
                Ok(record) => {
                    let kind = match a.kind {
                        crate::channel::AttachmentKind::Audio => MediaKind::Audio,
                        crate::channel::AttachmentKind::Image => MediaKind::Image,
                        crate::channel::AttachmentKind::Document => MediaKind::Document,
                    };
                    result.push(MessageAttachment {
                        attachment_id: record.id,
                        kind,
                        mime_type: a.mime_type.clone(),
                        filename: a.filename.clone(),
                    });
                }
                Err(e) => {
                    warn!("Failed to persist attachment: {}", e);
                }
            }
        }
        result
    }

    pub async fn get_bytes(&self, id: &str) -> Result<Vec<u8>> {
        let record = self.db.get_attachment(id.to_string()).await?
            .with_context(|| format!("Attachment not found: {}", id))?;

        let ext = mime_to_ext(&record.mime_type);
        let key = format!("{}.{}", id, ext);
        let path = self.storage_root.join(&key);

        tokio::fs::read(&path).await
            .with_context(|| format!("Failed to read attachment from {}", path.display()))
    }

    pub async fn get_base64(&self, id: &str) -> Result<String> {
        let bytes = self.get_bytes(id).await?;
        Ok(STANDARD.encode(&bytes))
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        if let Some(record) = self.db.get_attachment(id.to_string()).await? {
            let ext = mime_to_ext(&record.mime_type);
            let key = format!("{}.{}", id, ext);
            let path = self.storage_root.join(&key);
            let _ = tokio::fs::remove_file(&path).await;
        }
        self.db.delete_attachment(id.to_string()).await?;
        Ok(())
    }
}

fn kind_to_str(kind: &crate::channel::AttachmentKind) -> &'static str {
    use crate::channel::AttachmentKind;
    match kind {
        AttachmentKind::Audio => "audio",
        AttachmentKind::Image => "image",
        AttachmentKind::Document => "document",
    }
}

fn mime_to_ext(mime: &str) -> &str {
    let base = mime.split(';').next().unwrap_or(mime).trim();
    match base {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/ogg" => "ogg",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/webm" => "weba",
        "audio/mp4" | "audio/m4a" => "m4a",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "text/html" => "html",
        "application/json" => "json",
        _ => "bin",
    }
}