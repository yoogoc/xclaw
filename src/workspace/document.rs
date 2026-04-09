//! Memory document types for the workspace.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Well-known document paths.
///
/// These are conventional paths that have special meaning in the workspace.
/// Agents can create arbitrary paths beyond these.
pub mod paths {
    /// Long-term curated memory.
    pub const MEMORY: &str = "MEMORY.md";
    /// Agent identity (name, nature, vibe).
    pub const IDENTITY: &str = "IDENTITY.md";
    /// Core values and principles.
    pub const SOUL: &str = "SOUL.md";
    /// Behavior instructions.
    pub const AGENTS: &str = "AGENTS.md";
    /// User context (name, preferences).
    pub const USER: &str = "USER.md";
    /// Periodic checklist for heartbeat.
    pub const HEARTBEAT: &str = "HEARTBEAT.md";
    /// Root runbook/readme.
    pub const README: &str = "README.md";
    /// Daily logs directory.
    pub const DAILY_DIR: &str = "daily/";
    /// Context directory (for identity-related docs).
    pub const CONTEXT_DIR: &str = "context/";
    /// User-editable notes for environment-specific tool guidance.
    pub const TOOLS: &str = "TOOLS.md";
    /// First-run ritual file; self-deletes after onboarding completes.
    pub const BOOTSTRAP: &str = "BOOTSTRAP.md";
}

/// A memory document stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDocument {
    /// Unique document ID.
    pub id: Uuid,
    /// User identifier.
    pub user_id: String,
    /// Optional agent ID for multi-agent isolation.
    pub agent_id: Option<Uuid>,
    /// File path within the workspace (e.g., "context/vision.md").
    pub path: String,
    /// Full document content.
    pub content: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Flexible metadata.
    pub metadata: serde_json::Value,
}

impl MemoryDocument {
    /// Create a new document with a path.
    pub fn new(
        user_id: impl Into<String>,
        agent_id: Option<Uuid>,
        path: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            agent_id,
            path: path.into(),
            content: String::new(),
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Get the file name from the path.
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Get the parent directory from the path.
    pub fn parent_dir(&self) -> Option<&str> {
        let idx = self.path.rfind('/')?;
        Some(&self.path[..idx])
    }

    /// Check if the document is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get word count.
    pub fn word_count(&self) -> usize {
        self.content.split_whitespace().count()
    }

    /// Check if this is a well-known identity document.
    pub fn is_identity_document(&self) -> bool {
        matches!(
            self.path.as_str(),
            paths::IDENTITY | paths::SOUL | paths::AGENTS | paths::USER
        )
    }
}

/// An entry in a workspace directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    /// Path relative to listing directory.
    pub path: String,
    /// True if this is a directory (has children).
    pub is_directory: bool,
    /// Last update timestamp (latest among children for directories).
    pub updated_at: Option<DateTime<Utc>>,
    /// Preview of content (first ~200 chars, None for directories).
    pub content_preview: Option<String>,
}

impl WorkspaceEntry {
    /// Get the entry name (last path component).
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// A chunk of a memory document for search indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChunk {
    /// Unique chunk ID.
    pub id: Uuid,
    /// Parent document ID.
    pub document_id: Uuid,
    /// Position in the document (0-based).
    pub chunk_index: i32,
    /// Chunk text content.
    pub content: String,
    /// Embedding vector (if generated).
    pub embedding: Option<Vec<f32>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl MemoryChunk {
    /// Create a new chunk (not persisted yet).
    pub fn new(document_id: Uuid, chunk_index: i32, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            document_id,
            chunk_index,
            content: content.into(),
            embedding: None,
            created_at: Utc::now(),
        }
    }

    /// Set the embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}
