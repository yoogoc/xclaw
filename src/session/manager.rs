use crate::session::Session;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

const SESSION_COUNT_WARNING_THRESHOLD: usize = 1000;

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ChatId {
    DM,
    Room(String),
}

pub struct SessionManager {
    sessions: RwLock<HashMap<ChatId, Arc<Mutex<Session>>>>,
    thread_map: RwLock<HashMap<ChatId, Uuid>>,
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
    /// Get or create a session for a user.
    pub async fn get_or_create_session(&self, chat_id: &ChatId) -> Arc<Mutex<Session>> {
        // Fast path: check if session exists
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(chat_id) {
                return Arc::clone(session);
            }
        }

        // Slow path: create new session
        let mut sessions = self.sessions.write().await;
        // Double-check after acquiring write lock
        if let Some(session) = sessions.get(chat_id) {
            return Arc::clone(session);
        }

        let new_session = Session::new(chat_id.clone());
        let session_id = new_session.id.to_string();
        let session = Arc::new(Mutex::new(new_session));
        sessions.insert(chat_id.clone(), Arc::clone(&session));

        if sessions.len() >= SESSION_COUNT_WARNING_THRESHOLD && sessions.len() % 100 == 0 {
            warn!(
                "High session count: {} active sessions. \
                 Pruning runs every 10 minutes; consider reducing session_idle_timeout.",
                sessions.len()
            );
        }

        session
    }

    /// Resolve an external thread ID to an internal thread.
    ///
    /// Returns the session and thread ID. Creates both if they don't exist.
    pub async fn resolve_thread(
        &self,
        chat_id: &ChatId,
        external_thread_id: Option<&str>,
    ) -> (Arc<Mutex<Session>>, Uuid) {
        let session = self.get_or_create_session(chat_id).await;

        // Check if we have a mapping
        {
            let thread_map = self.thread_map.read().await;
            if let Some(&thread_id) = thread_map.get(chat_id) {
                // Verify thread still exists in session
                let sess = session.lock().await;
                if sess.threads.contains_key(&thread_id) {
                    return (Arc::clone(&session), thread_id);
                }
            }
        }

        // Create new thread (always create a new one for a new key)
        let thread_id = {
            let mut sess = session.lock().await;
            let thread = sess.create_thread();
            thread.id
        };

        // Store mapping
        {
            let mut thread_map = self.thread_map.write().await;
            thread_map.insert(chat_id.clone(), thread_id);
        }

        (session, thread_id)
    }

    /// Register a hydrated thread so subsequent `resolve_thread` calls find it.
    ///
    /// Inserts into the thread_map and creates an undo manager for the thread.
    pub async fn register_thread(
        &self,
        chat_id: &ChatId,
        thread_id: Uuid,
        session: Arc<Mutex<Session>>,
    ) {
        {
            let mut thread_map = self.thread_map.write().await;
            thread_map.insert(chat_id.clone(), thread_id);
        }

        // Ensure the session is tracked
        {
            let mut sessions = self.sessions.write().await;
            sessions.entry(chat_id.clone()).or_insert(session);
        }
    }

    /// Remove sessions that have been idle for longer than the given duration.
    ///
    /// Returns the number of sessions pruned.
    pub async fn prune_stale_sessions(&self, max_idle: std::time::Duration) -> usize {
        let cutoff = chrono::Utc::now() - chrono::TimeDelta::seconds(max_idle.as_secs() as i64);

        // Find stale sessions (chat_id + session_id)
        let stale_sessions: Vec<(ChatId, String)> = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .filter_map(|(chat_id, session)| {
                    // Try to lock; skip if contended (someone is actively using it)
                    let sess = session.try_lock().ok()?;
                    if sess.last_active_at < cutoff {
                        Some((chat_id.clone(), sess.id.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        };

        let stale_chats: Vec<ChatId> = stale_sessions
            .iter()
            .map(|(chat_id, _)| chat_id.clone())
            .collect();

        if stale_chats.is_empty() {
            return 0;
        }

        // Collect thread IDs from stale sessions for cleanup
        let mut stale_thread_ids: Vec<Uuid> = Vec::new();
        {
            let sessions = self.sessions.read().await;
            for chat_id in &stale_chats {
                if let Some(session) = sessions.get(chat_id)
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
            for chat_id in &stale_chats {
                sessions.remove(chat_id);
            }
            before - sessions.len()
        };

        // Clean up thread mappings that point to stale sessions
        {
            let mut thread_map = self.thread_map.write().await;
            thread_map.retain(|key, _| !stale_chats.contains(&key));
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
