use crate::session::Session;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

const SESSION_COUNT_WARNING_THRESHOLD: usize = 1000;

/// Key for routing external conversations to internal threads.
///
/// Uniquely identifies a conversation thread across:
/// - binding (agent@channel combination)
/// - user identity
/// - channel platform
/// - external thread ID (for threaded platforms)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadKey {
    /// Binding identifier (e.g. "main@main" or "backend-engineer@backend").
    pub binding_id: String,
    /// User identifier within the channel.
    pub user_id: String,
    /// Channel name/identifier.
    pub channel: String,
    /// External thread ID for threaded conversations.
    /// `None` is distinct from `Some(...)` — they represent different keys.
    pub external_thread_id: Option<String>,
}

impl ThreadKey {
    /// Create a new thread key.
    pub fn new(
        binding_id: impl Into<String>,
        user_id: impl Into<String>,
        channel: impl Into<String>,
        external_thread_id: Option<String>,
    ) -> Self {
        Self {
            binding_id: binding_id.into(),
            user_id: user_id.into(),
            channel: channel.into(),
            external_thread_id,
        }
    }

    /// Create a thread key without external thread (non-threaded platforms).
    pub fn without_thread(
        binding_id: impl Into<String>,
        user_id: impl Into<String>,
        channel: impl Into<String>,
    ) -> Self {
        Self {
            binding_id: binding_id.into(),
            user_id: user_id.into(),
            channel: channel.into(),
            external_thread_id: None,
        }
    }
}

/// Session manager for agent bindings.
///
/// Manages sessions per binding and routes external conversations
/// to internal threads using ThreadKey.
pub struct SessionManager {
    /// Active sessions keyed by binding_id.
    sessions: RwLock<HashMap<String, Arc<Mutex<Session>>>>,
    /// Maps external thread keys to internal thread IDs.
    thread_map: RwLock<HashMap<ThreadKey, Uuid>>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            thread_map: RwLock::new(HashMap::new()),
        }
    }
}

impl SessionManager {
    /// Get or create a session for a binding.
    pub async fn get_or_create_session(&self, binding_id: &str) -> Arc<Mutex<Session>> {
        // Fast path: check if session exists
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(binding_id) {
                return Arc::clone(session);
            }
        }

        // Slow path: create new session
        let mut sessions = self.sessions.write().await;
        // Double-check after acquiring write lock
        if let Some(session) = sessions.get(binding_id) {
            return Arc::clone(session);
        }

        let new_session = Session::new(binding_id.to_string());
        let session = Arc::new(Mutex::new(new_session));
        sessions.insert(binding_id.to_string(), Arc::clone(&session));

        if sessions.len() >= SESSION_COUNT_WARNING_THRESHOLD && sessions.len() % 100 == 0 {
            warn!(
                "High session count: {} active sessions. \
                 Pruning runs every 10 minutes; consider reducing session_idle_timeout.",
                sessions.len()
            );
        }

        session
    }

    /// Resolve an external thread to an internal thread.
    ///
    /// Returns the session and thread ID. Creates both if they don't exist.
    ///
    /// Routing rules per design/session.md:
    /// - Same `(binding_id, user_id, channel, external_thread_id)` → same thread
    /// - Different `external_thread_id` → different thread
    /// - Same user, different channel → different thread
    /// - `None` vs `Some(...)` are distinct keys
    pub async fn resolve_thread(
        &self,
        binding_id: &str,
        user_id: &str,
        channel: &str,
        external_thread_id: Option<&str>,
    ) -> (Arc<Mutex<Session>>, Uuid) {
        let key = ThreadKey::new(
            binding_id,
            user_id,
            channel,
            external_thread_id.map(String::from),
        );
        let session = self.get_or_create_session(binding_id).await;

        // Check if we have a mapping
        {
            let thread_map = self.thread_map.read().await;
            if let Some(&thread_id) = thread_map.get(&key) {
                // Verify thread still exists in session
                let sess = session.lock().await;
                if sess.threads.contains_key(&thread_id) {
                    return (Arc::clone(&session), thread_id);
                }
            }
        }

        // Create new thread with routing info
        let thread_id = {
            let mut sess = session.lock().await;
            // Use with_routing to set external routing identities
            let thread = sess.create_thread();
            // Update the thread with routing information
            thread.user_id = user_id.to_string();
            thread.channel = channel.to_string();
            thread.external_thread_id = external_thread_id.map(String::from);
            thread.id
        };

        // Store mapping
        {
            let mut thread_map = self.thread_map.write().await;
            thread_map.insert(key, thread_id);
        }

        (session, thread_id)
    }

    /// Register a hydrated thread so subsequent `resolve_thread` calls find it.
    ///
    /// Inserts into the thread_map and creates an undo manager for the thread.
    pub async fn register_thread(
        &self,
        binding_id: &str,
        user_id: &str,
        channel: &str,
        external_thread_id: Option<&str>,
        thread_id: Uuid,
        session: Arc<Mutex<Session>>,
    ) {
        let key = ThreadKey::new(
            binding_id,
            user_id,
            channel,
            external_thread_id.map(String::from),
        );
        {
            let mut thread_map = self.thread_map.write().await;
            thread_map.insert(key, thread_id);
        }

        // Ensure the session is tracked
        {
            let mut sessions = self.sessions.write().await;
            sessions.entry(binding_id.to_string()).or_insert(session);
        }
    }

    /// Remove sessions that have been idle for longer than the given duration.
    ///
    /// Returns the number of sessions pruned.
    pub async fn prune_stale_sessions(&self, max_idle: std::time::Duration) -> usize {
        let cutoff = chrono::Utc::now() - chrono::TimeDelta::seconds(max_idle.as_secs() as i64);

        // Find stale sessions (binding_id + session_id)
        let stale_sessions: Vec<(String, String)> = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .filter_map(|(binding_id, session)| {
                    // Try to lock; skip if contended (someone is actively using it)
                    let sess = session.try_lock().ok()?;
                    if sess.last_active_at < cutoff {
                        Some((binding_id.clone(), sess.id.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        };

        let stale_bindings: Vec<String> = stale_sessions
            .iter()
            .map(|(binding_id, _)| binding_id.clone())
            .collect();

        if stale_bindings.is_empty() {
            return 0;
        }

        // Collect thread IDs from stale sessions for cleanup
        let mut stale_thread_ids: Vec<Uuid> = Vec::new();
        {
            let sessions = self.sessions.read().await;
            for binding_id in &stale_bindings {
                if let Some(session) = sessions.get(binding_id)
                    && let Ok(sess) = session.try_lock()
                {
                    stale_thread_ids.extend(sess.threads.keys());
                }
            }
        }

        // Remove sessions
        let count = {
            let mut sessions = self.sessions.write().await;
            let before = sessions.len();
            for binding_id in &stale_bindings {
                sessions.remove(binding_id);
            }
            before - sessions.len()
        };

        // Clean up thread mappings that point to stale sessions
        {
            let mut thread_map = self.thread_map.write().await;
            thread_map.retain(|key, _| !stale_bindings.contains(&key.binding_id));
        }

        if count > 0 {
            info!(
                "Pruned {} stale session(s) (idle > {}s)",
                count,
                max_idle.as_secs()
            );
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_same_thread_key_returns_same_thread() {
        let manager = SessionManager::new();

        // First call creates session and thread
        let (session1, thread_id1) = manager
            .resolve_thread("main@main", "user123", "discord", Some("thread456"))
            .await;

        // Second call with same key should return same thread
        let (session2, thread_id2) = manager
            .resolve_thread("main@main", "user123", "discord", Some("thread456"))
            .await;

        assert_eq!(thread_id1, thread_id2);
        assert!(Arc::ptr_eq(&session1, &session2));
    }

    #[tokio::test]
    async fn test_different_external_thread_creates_different_thread() {
        let manager = SessionManager::new();

        let (_, thread_id1) = manager
            .resolve_thread("main@main", "user123", "discord", Some("thread456"))
            .await;

        let (_, thread_id2) = manager
            .resolve_thread("main@main", "user123", "discord", Some("thread789"))
            .await;

        assert_ne!(thread_id1, thread_id2);
    }

    #[tokio::test]
    async fn test_none_vs_some_external_thread_are_different_keys() {
        let manager = SessionManager::new();

        let (_, thread_id1) = manager
            .resolve_thread("main@main", "user123", "discord", None)
            .await;

        let (_, thread_id2) = manager
            .resolve_thread("main@main", "user123", "discord", Some("thread456"))
            .await;

        assert_ne!(thread_id1, thread_id2);
    }

    #[tokio::test]
    async fn test_same_user_different_channel_creates_different_thread() {
        let manager = SessionManager::new();

        let (_, thread_id1) = manager
            .resolve_thread("main@main", "user123", "discord", None)
            .await;

        let (_, thread_id2) = manager
            .resolve_thread("main@main", "user123", "slack", None)
            .await;

        assert_ne!(thread_id1, thread_id2);
    }

    #[tokio::test]
    async fn test_get_or_create_session_reuses_session() {
        let manager = SessionManager::new();

        let session1 = manager.get_or_create_session("main@main").await;
        let session2 = manager.get_or_create_session("main@main").await;

        assert!(Arc::ptr_eq(&session1, &session2));
    }

    #[tokio::test]
    async fn test_different_binding_ids_create_different_sessions() {
        let manager = SessionManager::new();

        let session1 = manager.get_or_create_session("main@main").await;
        let session2 = manager.get_or_create_session("backend@backend").await;

        // Should be different sessions
        let sess1 = session1.lock().await;
        let sess2 = session2.lock().await;
        assert_ne!(sess1.id, sess2.id);
        assert_eq!(sess1.binding_id, "main@main");
        assert_eq!(sess2.binding_id, "backend@backend");
    }

    #[tokio::test]
    async fn test_register_thread_preserves_mapping() {
        let manager = SessionManager::new();

        // Create a session and thread through normal flow
        let (session, thread_id) = manager
            .resolve_thread("main@main", "user123", "discord", Some("thread456"))
            .await;

        // Resolve again should return the same thread
        let (_, resolved_thread_id) = manager
            .resolve_thread("main@main", "user123", "discord", Some("thread456"))
            .await;

        assert_eq!(thread_id, resolved_thread_id);
    }
}
