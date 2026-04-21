pub mod convert;
pub mod models;
pub mod schema;

use std::collections::HashMap;
use std::collections::HashSet;

use anyhow::{Context, Result};
use diesel::SqliteConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use uuid::Uuid;

use crate::session::{Session, Thread, Turn, TurnToolCall};

use self::convert::*;
use self::models::*;
use self::schema::*;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub struct Database {
    pool: Pool<ConnectionManager<SqliteConnection>>,
}

impl Database {
    pub fn new(database_url: &str) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(database_url).parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let manager = ConnectionManager::<SqliteConnection>::new(database_url);
        let pool = Pool::builder()
            .max_size(4)
            .build(manager)
            .with_context(|| format!("Failed to create connection pool for {}", database_url))?;

        // Run pending migrations
        {
            let mut conn = pool.get().context("Failed to get connection for migrations")?;
            conn.run_pending_migrations(MIGRATIONS).map_err(|e| anyhow::anyhow!("Migration failed: {}", e))?;
        }

        info!("Database initialized: {}", database_url);
        Ok(Self { pool })
    }

    /// Run a blocking Diesel operation on the thread pool.
    async fn run<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = pool.get().context("Failed to get DB connection")?;
            f(&mut conn)
        })
        .await
        .context("spawn_blocking panicked")?
    }

    // ── Bulk Operations ──

    /// Load all sessions from DB (with threads, turns, tool_calls).
    pub async fn load_all_sessions(&self) -> Result<Vec<Session>> {
        self.run(|conn| {
            // Load all rows
            let session_rows = sessions::table.load::<SessionRow>(conn).context("Failed to load sessions")?;

            let thread_rows = threads::table.load::<ThreadRow>(conn).context("Failed to load threads")?;

            let turn_rows = turns::table.order(turns::turn_number.asc()).load::<TurnRow>(conn).context("Failed to load turns")?;

            let tool_call_rows = turn_tool_calls::table
                .order(turn_tool_calls::call_index.asc())
                .load::<ToolCallRow>(conn)
                .context("Failed to load tool calls")?;

            // Group tool_calls by turn_id
            let mut tc_by_turn: HashMap<String, Vec<TurnToolCall>> = HashMap::new();
            for row in tool_call_rows {
                tc_by_turn.entry(row.turn_id.clone()).or_default().push(tool_call_from_row(row));
            }

            // Group turns by thread_id
            let mut turns_by_thread: HashMap<String, Vec<Turn>> = HashMap::new();
            for row in turn_rows {
                let turn_id = row.id.clone();
                let tool_calls = tc_by_turn.remove(&turn_id).unwrap_or_default();
                let turn = turn_from_row(row, tool_calls)?;
                turns_by_thread.entry(turn.thread_id.to_string()).or_default().push(turn);
            }

            // Group threads by session_id
            let mut threads_by_session: HashMap<String, HashMap<Uuid, Thread>> = HashMap::new();
            for row in thread_rows {
                let thread_id_str = row.id.clone();
                let session_id_str = row.session_id.clone();
                let turns = turns_by_thread.remove(&thread_id_str).unwrap_or_default();
                let thread = thread_from_row(row, turns)?;
                threads_by_session.entry(session_id_str).or_default().insert(thread.id, thread);
            }

            // Assemble sessions
            let mut result = Vec::with_capacity(session_rows.len());
            for row in session_rows {
                let session_id_str = row.id.clone();
                let threads = threads_by_session.remove(&session_id_str).unwrap_or_default();
                let session = session_from_row(row, threads)?;
                result.push(session);
            }

            Ok(result)
        })
        .await
    }

    /// Save a complete session tree (used for startup state fixup).
    pub async fn save_session_full(&self, session: Session) -> Result<()> {
        self.run(move |conn| {
            conn.transaction(|conn| {
                // Insert session
                let sv = SessionInsertValues::from_session(&session);
                diesel::insert_into(sessions::table).values(&sv.as_new_row()).on_conflict(sessions::id).do_nothing().execute(conn)?;

                // Insert threads + turns + tool_calls
                for thread in session.threads.values() {
                    let tv = ThreadInsertValues::from_thread(thread);
                    diesel::insert_into(threads::table).values(&tv.as_new_row()).on_conflict(threads::id).do_nothing().execute(conn)?;

                    for turn in &thread.turns {
                        let uv = TurnInsertValues::from_turn(turn);
                        diesel::insert_into(turns::table).values(&uv.as_new_row()).on_conflict(turns::id).do_nothing().execute(conn)?;

                        let turn_id_str = turn.id.to_string();
                        for (i, tc) in turn.tool_calls.iter().enumerate() {
                            let cv = ToolCallInsertValues::from_tool_call(&turn_id_str, i, tc);
                            diesel::insert_into(turn_tool_calls::table).values(&cv.as_new_row()).on_conflict_do_nothing().execute(conn)?;
                        }
                    }
                }

                Ok(())
            })
        })
        .await
    }

    // ── Session ──

    pub async fn insert_session(&self, session: Session) -> Result<()> {
        self.run(move |conn| {
            let sv = SessionInsertValues::from_session(&session);
            diesel::insert_into(sessions::table).values(&sv.as_new_row()).execute(conn).context("Failed to insert session")?;
            Ok(())
        })
        .await
    }

    pub async fn delete_session(&self, session_id: String) -> Result<()> {
        self.run(move |conn| {
            diesel::delete(sessions::table.filter(sessions::id.eq(&session_id))).execute(conn).context("Failed to delete session")?;
            Ok(())
        })
        .await
    }

    pub async fn update_session_active_thread(&self, session_id: String, thread_id: String) -> Result<()> {
        self.run(move |conn| {
            diesel::update(sessions::table.filter(sessions::id.eq(&session_id)))
                .set(sessions::active_thread_id.eq(Some(&thread_id)))
                .execute(conn)
                .context("Failed to update session active_thread_id")?;
            Ok(())
        })
        .await
    }

    pub async fn update_session_auto_approved_tools(&self, session_id: String, tools: HashSet<String>) -> Result<()> {
        self.run(move |conn| {
            let tools_json = serde_json::to_string(&tools).unwrap_or_else(|_| "[]".to_string());
            diesel::update(sessions::table.filter(sessions::id.eq(&session_id)))
                .set(sessions::auto_approved_tools.eq(&tools_json))
                .execute(conn)
                .context("Failed to update session auto_approved_tools")?;
            Ok(())
        })
        .await
    }

    // ── Thread ──

    pub async fn insert_thread(&self, thread: Thread) -> Result<()> {
        self.run(move |conn| {
            let tv = ThreadInsertValues::from_thread(&thread);
            diesel::insert_into(threads::table).values(&tv.as_new_row()).execute(conn).context("Failed to insert thread")?;
            Ok(())
        })
        .await
    }

    pub async fn update_thread_state(&self, thread_id: String, state: String, updated_at: String) -> Result<()> {
        self.run(move |conn| {
            diesel::update(threads::table.filter(threads::id.eq(&thread_id)))
                .set((threads::state.eq(&state), threads::updated_at.eq(&updated_at)))
                .execute(conn)
                .context("Failed to update thread state")?;
            Ok(())
        })
        .await
    }

    pub async fn update_thread_pending_approvals(&self, thread_id: String, state: String, pending_approvals: String, updated_at: String) -> Result<()> {
        self.run(move |conn| {
            diesel::update(threads::table.filter(threads::id.eq(&thread_id)))
                .set((threads::state.eq(&state), threads::pending_approvals.eq(&pending_approvals), threads::updated_at.eq(&updated_at)))
                .execute(conn)
                .context("Failed to update thread pending_approvals")?;
            Ok(())
        })
        .await
    }

    // ── Turn ──

    pub async fn insert_turn(&self, turn: Turn) -> Result<()> {
        self.run(move |conn| {
            let uv = TurnInsertValues::from_turn(&turn);
            diesel::insert_into(turns::table).values(&uv.as_new_row()).execute(conn).context("Failed to insert turn")?;
            Ok(())
        })
        .await
    }

    pub async fn complete_turn(&self, turn_id: String, response: Option<String>, thinking: Option<String>, completed_at: String) -> Result<()> {
        self.run(move |conn| {
            diesel::update(turns::table.filter(turns::id.eq(&turn_id)))
                .set((
                    turns::state.eq("Completed"),
                    turns::response.eq(response.as_deref()),
                    turns::thinking.eq(thinking.as_deref()),
                    turns::completed_at.eq(Some(&completed_at)),
                ))
                .execute(conn)
                .context("Failed to complete turn")?;
            Ok(())
        })
        .await
    }

    pub async fn fail_turn(&self, turn_id: String, error: String, completed_at: String) -> Result<()> {
        self.run(move |conn| {
            diesel::update(turns::table.filter(turns::id.eq(&turn_id)))
                .set((turns::state.eq("Failed"), turns::error.eq(Some(&error)), turns::completed_at.eq(Some(&completed_at))))
                .execute(conn)
                .context("Failed to fail turn")?;
            Ok(())
        })
        .await
    }

    pub async fn interrupt_turn(&self, turn_id: String, completed_at: String) -> Result<()> {
        self.run(move |conn| {
            diesel::update(turns::table.filter(turns::id.eq(&turn_id)))
                .set((turns::state.eq("Interrupted"), turns::completed_at.eq(Some(&completed_at))))
                .execute(conn)
                .context("Failed to interrupt turn")?;
            Ok(())
        })
        .await
    }

    // ── ToolCall ──

    pub async fn insert_tool_call(&self, turn_id: String, index: usize, call: TurnToolCall) -> Result<()> {
        self.run(move |conn| {
            let cv = ToolCallInsertValues::from_tool_call(&turn_id, index, &call);
            diesel::insert_into(turn_tool_calls::table)
                .values(&cv.as_new_row())
                .execute(conn)
                .context("Failed to insert tool call")?;
            Ok(())
        })
        .await
    }

    pub async fn update_tool_call_result(&self, turn_id: String, index: usize, result: Option<String>, error: Option<String>) -> Result<()> {
        self.run(move |conn| {
            diesel::update(turn_tool_calls::table.filter(turn_tool_calls::turn_id.eq(&turn_id).and(turn_tool_calls::call_index.eq(index as i32))))
                .set((turn_tool_calls::result.eq(result.as_deref()), turn_tool_calls::error.eq(error.as_deref())))
                .execute(conn)
                .context("Failed to update tool call result")?;
            Ok(())
        })
        .await
    }

    // ── Attachment ──

    pub async fn insert_attachment(&self, id: String, kind: String, mime_type: String, filename: Option<String>, size_bytes: Option<i32>, source_url: Option<String>, created_at: String) -> Result<()> {
        self.run(move |conn| {
            let row = NewAttachmentRow {
                id: &id,
                kind: &kind,
                mime_type: &mime_type,
                filename: filename.as_deref(),
                size_bytes,
                source_url: source_url.as_deref(),
                created_at: &created_at,
            };
            diesel::insert_into(attachments::table)
                .values(&row)
                .execute(conn)
                .context("Failed to insert attachment")?;
            Ok(())
        })
        .await
    }

    pub async fn get_attachment(&self, id: String) -> Result<Option<AttachmentRow>> {
        self.run(move |conn| {
            let result = attachments::table
                .filter(attachments::id.eq(&id))
                .first::<AttachmentRow>(conn)
                .optional()
                .context("Failed to query attachment")?;
            Ok(result)
        })
        .await
    }

    pub async fn delete_attachment(&self, id: String) -> Result<()> {
        self.run(move |conn| {
            diesel::delete(attachments::table.filter(attachments::id.eq(&id)))
                .execute(conn)
                .context("Failed to delete attachment")?;
            Ok(())
        })
        .await
    }
}
