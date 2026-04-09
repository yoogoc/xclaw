use crate::session::{Session, ThreadState, Turn, TurnState, TurnToolCall};
use crate::storage::Database;
use crate::storage::convert::thread_state_to_str;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    pub fn new(binding_id: impl Into<String>, user_id: impl Into<String>, channel: impl Into<String>, external_thread_id: Option<String>) -> Self {
        Self {
            binding_id: binding_id.into(),
            user_id: user_id.into(),
            channel: channel.into(),
            external_thread_id,
        }
    }

    /// Create a thread key without external thread (non-threaded platforms).
    pub fn without_thread(binding_id: impl Into<String>, user_id: impl Into<String>, channel: impl Into<String>) -> Self {
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
    /// Optional database for persistence.
    db: Option<Arc<Database>>,
}

impl SessionManager {
    /// Create a new session manager (in-memory only, no persistence).
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            thread_map: RwLock::new(HashMap::new()),
            db: None,
        }
    }

    /// Create a new session manager backed by a database.
    /// Loads all sessions from DB and fixes interrupted state.
    pub async fn new_with_db(db: Arc<Database>) -> anyhow::Result<Self> {
        let mut all_sessions = db.load_all_sessions().await?;

        let mut sessions_map: HashMap<String, Arc<Mutex<Session>>> = HashMap::new();
        let mut thread_map: HashMap<ThreadKey, Uuid> = HashMap::new();

        for session in &mut all_sessions {
            // Fix interrupted state on restart
            for thread in session.threads.values_mut() {
                // Processing threads -> Idle (execution was interrupted by restart)
                if thread.state == ThreadState::Processing {
                    thread.state = ThreadState::Idle;
                    thread.updated_at = chrono::Utc::now();
                }

                for turn in &mut thread.turns {
                    // Processing turns -> Interrupted (they can't resume)
                    if turn.state == TurnState::Processing {
                        turn.state = TurnState::Interrupted;
                        turn.completed_at = Some(chrono::Utc::now());
                    }
                }

                // Rebuild thread_map
                let key = ThreadKey::new(&session.binding_id, &thread.user_id, &thread.channel, thread.external_thread_id.clone());
                thread_map.insert(key, thread.id);
            }

            let binding_id = session.binding_id.clone();
            sessions_map.insert(binding_id, Arc::new(Mutex::new(session.clone())));
        }

        info!("Loaded {} session(s) from database", sessions_map.len());

        Ok(Self {
            sessions: RwLock::new(sessions_map),
            thread_map: RwLock::new(thread_map),
            db: Some(db),
        })
    }

    /// Helper: run a persistence operation, logging errors without blocking.
    async fn persist<F, Fut>(&self, f: F)
    where
        F: FnOnce(Arc<Database>) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        if let Some(ref db) = self.db {
            if let Err(e) = f(db.clone()).await {
                error!("Persist failed: {}", e);
            }
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
        let session_clone = new_session.clone();
        let session = Arc::new(Mutex::new(new_session));
        sessions.insert(binding_id.to_string(), Arc::clone(&session));

        if sessions.len() >= SESSION_COUNT_WARNING_THRESHOLD && sessions.len() % 100 == 0 {
            warn!(
                "High session count: {} active sessions. \
                 Pruning runs every 10 minutes; consider reducing session_idle_timeout.",
                sessions.len()
            );
        }

        // Drop the write lock before persisting
        drop(sessions);

        // Persist new session
        self.persist(|db| async move { db.insert_session(session_clone).await }).await;

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
    pub async fn resolve_thread(&self, binding_id: &str, user_id: &str, channel: &str, external_thread_id: Option<&str>) -> (Arc<Mutex<Session>>, Uuid) {
        let key = ThreadKey::new(binding_id, user_id, channel, external_thread_id.map(String::from));
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
        let (thread_id, thread_clone, session_id) = {
            let mut sess = session.lock().await;
            let session_id = sess.id.to_string();
            // Use with_routing to set external routing identities
            let thread = sess.create_thread();
            // Update the thread with routing information
            thread.user_id = user_id.to_string();
            thread.channel = channel.to_string();
            thread.external_thread_id = external_thread_id.map(String::from);
            let tid = thread.id;
            let tc = thread.clone();
            (tid, tc, session_id)
        };

        // Store mapping
        {
            let mut thread_map = self.thread_map.write().await;
            thread_map.insert(key, thread_id);
        }

        // Persist new thread + update session active_thread
        let thread_id_str = thread_id.to_string();
        let session_id_clone = session_id.clone();
        self.persist(|db| async move { db.insert_thread(thread_clone).await }).await;
        self.persist(|db| async move { db.update_session_active_thread(session_id_clone, thread_id_str).await }).await;

        (session, thread_id)
    }

    /// Register a hydrated thread so subsequent `resolve_thread` calls find it.
    ///
    /// Inserts into the thread_map and creates an undo manager for the thread.
    pub async fn register_thread(&self, binding_id: &str, user_id: &str, channel: &str, external_thread_id: Option<&str>, thread_id: Uuid, session: Arc<Mutex<Session>>) {
        let key = ThreadKey::new(binding_id, user_id, channel, external_thread_id.map(String::from));
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
                    if sess.last_active_at < cutoff { Some((binding_id.clone(), sess.id.to_string())) } else { None }
                })
                .collect()
        };

        let stale_bindings: Vec<String> = stale_sessions.iter().map(|(binding_id, _)| binding_id.clone()).collect();

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
            info!("Pruned {} stale session(s) (idle > {}s)", count, max_idle.as_secs());
        }

        // Persist deletions
        for (_, session_id) in &stale_sessions {
            let sid = session_id.clone();
            self.persist(|db| async move { db.delete_session(sid).await }).await;
        }

        count
    }
}

// ── Persistence wrappers for binding loop ──

impl SessionManager {
    /// Start a new turn on a thread, update in-memory state, and persist.
    ///
    /// Encapsulates: `thread.start_turn(user_input)` + DB `insert_turn()` + `update_thread_state()`.
    pub async fn thread_start_turn(&self, session: Arc<Mutex<Session>>, thread_id: Uuid, user_input: &str) {
        let mut sess = session.lock().await;
        if let Some(thread) = sess.threads.get_mut(&thread_id) {
            thread.start_turn(user_input);
            if let Some(turn) = thread.last_turn() {
                self.persist_turn_started(&turn.clone(), &thread.id.to_string(), &thread.updated_at.to_rfc3339()).await;
            }
        }
    }

    /// Complete the current turn on a thread, update in-memory state, and persist.
    ///
    /// Encapsulates: `thread.complete_turn(response)` + DB `complete_turn()` + `update_thread_state()`.
    pub async fn thread_complete_turn(&self, session: Arc<Mutex<Session>>, thread_id: Uuid, response: Option<String>) {
        let mut sess = session.lock().await;
        if let Some(thread) = sess.threads.get_mut(&thread_id) {
            thread.complete_turn(response.clone());
            if let Some(turn) = thread.last_turn() {
                self.persist_turn_completed(
                    &turn.id.to_string(),
                    response,
                    turn.thinking.clone(),
                    &turn.completed_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                    &thread.id.to_string(),
                    &thread.updated_at.to_rfc3339(),
                )
                .await;
            }
        }
    }

    /// Fail the current turn on a thread, update in-memory state, and persist.
    ///
    /// Encapsulates: `thread.fail_turn(error)` + DB `fail_turn()` + `update_thread_state()`.
    pub async fn thread_fail_turn(&self, session: Arc<Mutex<Session>>, thread_id: Uuid, error: &str) {
        let mut sess = session.lock().await;
        if let Some(thread) = sess.threads.get_mut(&thread_id) {
            thread.fail_turn(error);
            if let Some(turn) = thread.last_turn() {
                self.persist_turn_failed(
                    &turn.id.to_string(),
                    &turn.error.clone().unwrap_or_default(),
                    &turn.completed_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                    &thread.id.to_string(),
                    &thread.updated_at.to_rfc3339(),
                )
                .await;
            }
        }
    }

    // ── Private persist helpers ──

    async fn persist_turn_started(&self, turn: &Turn, thread_id: &str, updated_at: &str) {
        let turn_clone = turn.clone();
        let tid = thread_id.to_string();
        let ua = updated_at.to_string();
        self.persist(|db| async move { db.insert_turn(turn_clone).await }).await;
        self.persist(|db| async move { db.update_thread_state(tid, "Processing".to_string(), ua).await }).await;
    }

    async fn persist_turn_completed(&self, turn_id: &str, response: Option<String>, thinking: Option<String>, completed_at: &str, thread_id: &str, updated_at: &str) {
        let ti = turn_id.to_string();
        let ca = completed_at.to_string();
        let thid = thread_id.to_string();
        let ua = updated_at.to_string();
        self.persist(|db| async move { db.complete_turn(ti, response, thinking, ca).await }).await;
        self.persist(|db| async move { db.update_thread_state(thid, "Idle".to_string(), ua).await }).await;
    }

    async fn persist_turn_failed(&self, turn_id: &str, error: &str, completed_at: &str, thread_id: &str, updated_at: &str) {
        let ti = turn_id.to_string();
        let e = error.to_string();
        let ca = completed_at.to_string();
        let thid = thread_id.to_string();
        let ua = updated_at.to_string();
        self.persist(|db| async move { db.fail_turn(ti, e, ca).await }).await;
        self.persist(|db| async move { db.update_thread_state(thid, "Idle".to_string(), ua).await }).await;
    }

    /// Interrupt the current turn on a thread, update in-memory state, and persist.
    ///
    /// Encapsulates: `thread.interrupt()` + DB `interrupt_turn()` + `update_thread_state()`.
    /// Call sites only need to pass the session and thread_id.
    pub async fn thread_interrupt(&self, session: Arc<Mutex<Session>>, thread_id: Uuid) {
        let mut sess = session.lock().await;
        if let Some(thread) = sess.threads.get_mut(&thread_id) {
            thread.interrupt();
            if let Some(turn) = thread.last_turn() {
                self.persist_turn_interrupted(
                    &turn.id.to_string(),
                    &turn.completed_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                    &thread.id.to_string(),
                    &thread.updated_at.to_rfc3339(),
                )
                .await;
            }
        }
    }

    /// Persist an interrupted turn + thread state change to Interrupted.
    async fn persist_turn_interrupted(&self, turn_id: &str, completed_at: &str, thread_id: &str, updated_at: &str) {
        let ti = turn_id.to_string();
        let ca = completed_at.to_string();
        let thid = thread_id.to_string();
        let ua = updated_at.to_string();
        self.persist(|db| async move { db.interrupt_turn(ti, ca).await }).await;
        self.persist(|db| async move { db.update_thread_state(thid, "Interrupted".to_string(), ua).await }).await;
    }

    /// Persist a new tool call.
    pub async fn persist_tool_call(&self, turn_id: &str, index: usize, call: &TurnToolCall) {
        let ti = turn_id.to_string();
        let c = call.clone();
        self.persist(|db| async move { db.insert_tool_call(ti, index, c).await }).await;
    }

    /// Persist a tool call result/error.
    pub async fn persist_tool_result(&self, turn_id: &str, index: usize, result: Option<&serde_json::Value>, error: Option<&str>) {
        let ti = turn_id.to_string();
        let r = result.map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".to_string()));
        let e = error.map(String::from);
        self.persist(|db| async move { db.update_tool_call_result(ti, index, r, e).await }).await;
    }

    /// Persist auto-approved tools update.
    pub async fn persist_auto_approve(&self, session_id: &str, tools: &HashSet<String>) {
        let sid = session_id.to_string();
        let t = tools.clone();
        self.persist(|db| async move { db.update_session_auto_approved_tools(sid, t).await }).await;
    }

    /// Persist thread entering AwaitingApproval state.
    pub async fn persist_thread_awaiting_approval(&self, thread_id: &str, pending_approvals_json: &str, updated_at: &str) {
        let thid = thread_id.to_string();
        let pa = pending_approvals_json.to_string();
        let ua = updated_at.to_string();
        self.persist(|db| async move { db.update_thread_pending_approvals(thid, "AwaitingApproval".to_string(), pa, ua).await })
            .await;
    }

    /// Persist a generic thread state change.
    pub async fn persist_thread_state(&self, thread_id: &str, state: ThreadState, updated_at: &str) {
        let thid = thread_id.to_string();
        let s = thread_state_to_str(state).to_string();
        let ua = updated_at.to_string();
        self.persist(|db| async move { db.update_thread_state(thid, s, ua).await }).await;
    }
}
