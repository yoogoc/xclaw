# 会话持久化设计

## 1. 背景与目标

### 1.1 现状

当前 Session/Thread/Turn 全部存储在内存中（`SessionManager` 使用 `RwLock<HashMap>`），进程重启后所有会话历史丢失。Agent 无法在重启后恢复对话上下文，用户需要重新开始对话。

已有基础：

- Cargo.toml 已声明 `diesel = { version = "2.3.6", features = ["sqlite", "returning_clauses_for_sqlite_3_35"] }`
- `src/storage/mod.rs` 存在空壳 `pub struct Database {}`
- 所有 Session/Thread/Turn 结构体已实现 `Serialize` / `Deserialize`
- `Thread::restore_from_messages()` 已实现但未调用

### 1.2 目标

1. 会话数据（Session、Thread、Turn、ToolCall）持久化到 SQLite
2. 进程重启后自动恢复所有会话上下文
3. Agent 可以无缝继续之前的对话
4. 不改变现有内存数据模型，最小侵入

### 1.3 设计原则

- **内存为主，DB 为副**：内存状态始终是权威数据源，DB 是持久化副本
- **写穿透**：每次状态变更后同步写入 DB，不做延迟批量写入
- **启动加载**：启动时从 DB 恢复全部 session 到内存
- **容错降级**：DB 写入失败不阻断业务流程，仅记录错误日志
- **向后兼容**：`db` 字段为 `Option`，不配置数据库时行为不变

---

## 2. 数据库 Schema

共 4 张表，层级关系：`sessions` → `threads` → `turns` → `turn_tool_calls`。

### 2.1 sessions 表

存储 binding 级运行时上下文。

```sql
CREATE TABLE sessions (
    id            TEXT PRIMARY KEY NOT NULL,   -- UUID
    binding_id    TEXT NOT NULL UNIQUE,        -- 如 "main@main"
    active_thread_id TEXT,                     -- 当前活跃 thread UUID
    auto_approved_tools TEXT NOT NULL DEFAULT '[]',   -- JSON string array
    metadata      TEXT NOT NULL DEFAULT 'null',       -- JSON object
    created_at    TEXT NOT NULL,               -- ISO 8601 UTC
    last_active_at TEXT NOT NULL               -- ISO 8601 UTC
);

CREATE UNIQUE INDEX idx_sessions_binding ON sessions(binding_id);
```

**设计说明：**

- `binding_id` 加 UNIQUE 约束，一个 binding 最多一个 session
- `auto_approved_tools` 存为 JSON 数组字符串，如 `["file_read","list_dir"]`
- 时间戳统一用 ISO 8601 UTC 文本格式，SQLite 不原生支持时间类型

### 2.2 threads 表

存储一条连续对话流。

```sql
CREATE TABLE threads (
    id                 TEXT PRIMARY KEY NOT NULL,  -- UUID
    session_id         TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    user_id            TEXT NOT NULL,
    channel            TEXT NOT NULL,
    external_thread_id TEXT,                       -- 可空
    state              TEXT NOT NULL DEFAULT 'Idle',
    metadata           TEXT NOT NULL DEFAULT 'null',
    pending_approvals  TEXT NOT NULL DEFAULT '[]', -- JSON blob
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);

CREATE INDEX idx_threads_session ON threads(session_id);
CREATE INDEX idx_threads_routing ON threads(session_id, user_id, channel, external_thread_id);
```

**设计说明：**

- `state` 存枚举字符串：`Idle` | `Processing` | `AwaitingApproval` | `Completed` | `Interrupted`
- `pending_approvals` 整体序列化为 JSON blob（包含 `context_messages`），因为审批数据结构复杂且跨越多种类型，拆表收益不大
- 路由索引 `idx_threads_routing` 加速 `resolve_thread()` 的查找路径
- `ON DELETE CASCADE`：删除 session 时自动删除其所有 threads

### 2.3 turns 表

存储一轮完整 agentic run。

```sql
CREATE TABLE turns (
    id                      TEXT PRIMARY KEY NOT NULL,  -- UUID
    thread_id               TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    session_id              TEXT NOT NULL,              -- 冗余字段，方便查询
    turn_number             INTEGER NOT NULL,
    user_input              TEXT NOT NULL,
    thinking                TEXT,
    response                TEXT,
    state                   TEXT NOT NULL DEFAULT 'Processing',
    started_at              TEXT NOT NULL,
    completed_at            TEXT,
    error                   TEXT,
    current_tool_iterations INTEGER NOT NULL DEFAULT 0,
    draft_message_id        TEXT
);

CREATE INDEX idx_turns_thread ON turns(thread_id);
CREATE INDEX idx_turns_order  ON turns(thread_id, turn_number);
```

**设计说明：**

- `state` 存枚举字符串：`Processing` | `Completed` | `Failed` | `Interrupted`
- `session_id` 冗余存储，方便按 session 维度批量查询
- `turn_number` 配合 `thread_id` 保证排序
- **不存储 `image_content_parts`**：已标记 `#[serde(skip)]`，仅当前 LLM 调用需要，属瞬态数据

### 2.4 turn_tool_calls 表

存储 turn 中的每次工具调用。

```sql
CREATE TABLE turn_tool_calls (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    turn_id     TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    call_index  INTEGER NOT NULL,         -- 在 turn 中的顺序（0-based）
    name        TEXT NOT NULL,            -- 工具名
    parameters  TEXT NOT NULL DEFAULT '{}',  -- JSON
    result      TEXT,                     -- JSON，可空
    error       TEXT                      -- 可空
);

CREATE INDEX idx_toolcalls_turn ON turn_tool_calls(turn_id);
CREATE UNIQUE INDEX idx_toolcalls_order ON turn_tool_calls(turn_id, call_index);
```

**设计说明：**

- 自增 `id` 作为主键，`(turn_id, call_index)` 唯一约束
- `parameters`、`result` 均为 JSON 文本
- 级联删除：删除 turn 时自动清理其 tool calls

---

## 3. 持久化决策矩阵

| 字段 | 持久化 | 存储方式 | 理由 |
|------|--------|---------|------|
| Session 基本字段 | ✅ | 列 | session 上下文必须持久 |
| `auto_approved_tools` | ✅ | JSON 列 | session 级偏好 |
| Thread 基本字段 | ✅ | 列 | 对话流必须持久 |
| `pending_approvals` | ✅ | JSON blob 列 | 重启后可恢复审批流 |
| Turn 基本字段 | ✅ | 列 | 对话历史核心 |
| `image_content_parts` | ❌ | — | `#[serde(skip)]`，瞬态 |
| `draft_message_id` | ✅ | 列 | 重启后可尝试继续编辑 |
| TurnToolCall 全部字段 | ✅ | 列 | 工具调用历史，LLM 上下文重建所需 |

---

## 4. Storage 层实现

### 4.1 模块结构

```
src/storage/
├── mod.rs          -- Database 结构体 + 公开 API
├── models.rs       -- Diesel Queryable / Insertable 模型
├── schema.rs       -- diesel print-schema 自动生成
└── convert.rs      -- 内存模型 <-> DB 模型的转换
```

### 4.2 Database 结构体

```rust
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::SqliteConnection;

pub struct Database {
    pool: Pool<ConnectionManager<SqliteConnection>>,
}

impl Database {
    /// 创建连接池 + 自动运行迁移
    pub fn new(database_url: &str) -> Result<Self>;
}
```

连接池使用 `r2d2`（Diesel 内置支持），默认池大小为 4。

### 4.3 异步桥接

Diesel 不支持 async SQLite。通过 `spawn_blocking` 桥接：

```rust
impl Database {
    async fn run<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = pool.get()?;
            f(&mut conn)
        })
        .await?
    }
}
```

### 4.4 公开 API

```rust
impl Database {
    // ── 批量操作 ──

    /// 启动时加载所有 session（含 threads、turns、tool_calls）
    pub async fn load_all_sessions(&self) -> Result<Vec<Session>>;

    /// 保存完整 session 树（insert + update）
    pub async fn save_session_full(&self, session: &Session) -> Result<()>;

    // ── Session ──

    pub async fn insert_session(&self, session: &Session) -> Result<()>;
    pub async fn delete_session(&self, session_id: &str) -> Result<()>;

    /// 更新 active_thread_id（新建 thread 时调用）
    pub async fn update_session_active_thread(
        &self, session_id: &str, thread_id: &str,
    ) -> Result<()>;

    /// 更新 auto_approved_tools（用户选择"始终允许"时调用）
    pub async fn update_session_auto_approved_tools(
        &self, session_id: &str, tools: &HashSet<String>,
    ) -> Result<()>;

    // ── Thread ──

    pub async fn insert_thread(&self, thread: &Thread) -> Result<()>;

    /// 更新 thread 状态（Idle/Processing/Completed/Interrupted）
    pub async fn update_thread_state(
        &self, thread_id: &str, state: &str, updated_at: &str,
    ) -> Result<()>;

    /// 更新 pending_approvals（进入/退出审批时调用）
    pub async fn update_thread_pending_approvals(
        &self, thread_id: &str, state: &str,
        pending_approvals: &str, updated_at: &str,
    ) -> Result<()>;

    // ── Turn ──

    pub async fn insert_turn(&self, turn: &Turn) -> Result<()>;

    /// Turn 完成：设置 response、thinking、completed_at、state→Completed
    pub async fn complete_turn(
        &self, turn_id: &str, response: &str,
        thinking: Option<&str>, completed_at: &str,
    ) -> Result<()>;

    /// Turn 失败：设置 error、completed_at、state→Failed
    pub async fn fail_turn(
        &self, turn_id: &str, error: &str, completed_at: &str,
    ) -> Result<()>;

    /// Turn 中断：设置 completed_at、state→Interrupted
    pub async fn interrupt_turn(
        &self, turn_id: &str, completed_at: &str,
    ) -> Result<()>;

    // ── ToolCall ──

    pub async fn insert_tool_call(
        &self, turn_id: &str, index: usize, call: &TurnToolCall
    ) -> Result<()>;

    pub async fn update_tool_call_result(
        &self, turn_id: &str, index: usize,
        result: Option<&serde_json::Value>, error: Option<&str>
    ) -> Result<()>;
}
```

**写入语义**：创建时使用 `INSERT`，更新时使用特定字段的 `UPDATE ... SET field1=?, field2=? WHERE id = ?`，不使用 upsert，不使用通用 update 整行覆盖。

---

## 5. Diesel 模型

### 5.1 Queryable（读）

```rust
#[derive(Queryable, Selectable)]
#[diesel(table_name = sessions)]
pub struct SessionRow {
    pub id: String,
    pub binding_id: String,
    pub active_thread_id: Option<String>,
    pub auto_approved_tools: String,   // JSON
    pub metadata: String,              // JSON
    pub created_at: String,
    pub last_active_at: String,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = threads)]
pub struct ThreadRow {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub channel: String,
    pub external_thread_id: Option<String>,
    pub state: String,
    pub metadata: String,
    pub pending_approvals: String,     // JSON
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = turns)]
pub struct TurnRow {
    pub id: String,
    pub thread_id: String,
    pub session_id: String,
    pub turn_number: i32,
    pub user_input: String,
    pub thinking: Option<String>,
    pub response: Option<String>,
    pub state: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    pub current_tool_iterations: i32,
    pub draft_message_id: Option<String>,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = turn_tool_calls)]
pub struct ToolCallRow {
    pub id: i32,
    pub turn_id: String,
    pub call_index: i32,
    pub name: String,
    pub parameters: String,
    pub result: Option<String>,
    pub error: Option<String>,
}
```

### 5.2 Insertable（写）

每个 Row 对应一个 `NewXxxRow`（省略 auto-increment id），使用 `#[derive(Insertable)]`。

### 5.3 convert.rs 转换逻辑

负责 `Session ↔ SessionRow`、`Thread ↔ ThreadRow` 等双向转换：

- UUID ↔ String：`.to_string()` / `Uuid::parse_str()`
- DateTime ↔ String：`.to_rfc3339()` / `DateTime::parse_from_rfc3339()`
- HashSet ↔ JSON String：`serde_json::to_string()` / `serde_json::from_str()`
- ThreadState/TurnState ↔ String：手写 match 或 `serde_json` 序列化枚举

---

## 6. SessionManager 集成

### 6.1 结构变更

```rust
pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<Mutex<Session>>>>,
    thread_map: RwLock<HashMap<ThreadKey, Uuid>>,
    db: Option<Arc<Database>>,  // 新增
}
```

### 6.2 启动恢复

```rust
impl SessionManager {
    pub async fn new_with_db(db: Arc<Database>) -> Result<Self> {
        let manager = Self {
            sessions: RwLock::new(HashMap::new()),
            thread_map: RwLock::new(HashMap::new()),
            db: Some(db.clone()),
        };

        let all_sessions = db.load_all_sessions().await?;
        for session in all_sessions {
            let binding_id = session.binding_id.clone();

            // 重建 thread_map
            for (thread_id, thread) in &session.threads {
                let key = ThreadKey::new(
                    &binding_id, &thread.user_id,
                    &thread.channel, thread.external_thread_id.clone(),
                );
                manager.thread_map.write().await.insert(key, *thread_id);
            }

            manager.sessions.write().await.insert(
                binding_id,
                Arc::new(Mutex::new(session)),
            );
        }

        Ok(manager)
    }
}
```

### 6.3 重启状态修复

从 DB 加载后，修复中断的执行状态：

| 原状态 | 修复为 | 原因 |
|--------|--------|------|
| `ThreadState::Processing` | `ThreadState::Idle` | 执行被重启中断，不可恢复 |
| `TurnState::Processing` | `TurnState::Interrupted` | 标记为已中断 |
| `ThreadState::AwaitingApproval` | 保持不变 | 用户下次发消息可继续审批 |
| `ThreadState::Idle` | 保持不变 | 正常状态 |

### 6.4 写入时机

通过 `persist()` 辅助方法，在状态变更后异步写入：

```rust
impl SessionManager {
    async fn persist<F, Fut>(&self, f: F)
    where
        F: FnOnce(Arc<Database>) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        if let Some(ref db) = self.db {
            if let Err(e) = f(db.clone()).await {
                log::error!("持久化失败: {}", e);
            }
        }
    }
}
```

**写入点与操作对照表：**

| 触发事件 | 来源方法 | DB 操作 |
|---------|---------|---------|
| 创建新 session | `get_or_create_session()` | `insert_session()` |
| 创建新 thread | `resolve_thread()` | `insert_thread()` + `update_session_active_thread()` |
| 开始新 turn | `thread.start_turn()` | `insert_turn()` + `update_thread_state()` |
| 记录 tool call | `turn.record_tool_call()` | `insert_tool_call()` |
| 记录 tool result | `turn.record_tool_result()` | `update_tool_call_result()` |
| 完成 turn | `thread.complete_turn()` | `complete_turn()` + `update_thread_state()` |
| 失败 turn | `thread.fail_turn()` | `fail_turn()` + `update_thread_state()` |
| 中断 turn | `thread.interrupt()` | `interrupt_turn()` + `update_thread_state()` |
| 自动审批工具 | `session.auto_approve_tool()` | `update_session_auto_approved_tools()` |
| Thread 状态变更 | 各状态转换点 | `update_thread_state()` / `update_thread_pending_approvals()` |
| 清理过期 session | `prune_stale_sessions()` | `delete_session()` |

**各写入点详细说明：**

#### 1. 创建新 session — `insert_session()`

**触发场景**：某个 binding（如 `main@main`）首次收到消息，`SessionManager.get_or_create_session()` 在 `sessions` HashMap 中找不到对应 `binding_id`，创建新的 `Session` 实例。

**触发时序**：Binding Loop 启动 → 收到首条用户消息 → `get_or_create_session(binding_id)` → 未命中缓存 → 构建 Session（生成 UUID、初始化空 threads HashMap、设置 `created_at`/`last_active_at`）→ 插入内存 → `insert_session()` 写入 DB。

**写入数据**：`id`、`binding_id`、`active_thread_id`（初始为 NULL）、`auto_approved_tools`（初始 `[]`）、`metadata`（初始 `null`）、`created_at`、`last_active_at`。

#### 2. 创建新 thread — `insert_thread()`

**触发场景**：用户消息到达后，`resolve_thread()` 根据 `(binding_id, user_id, channel, external_thread_id)` 四元组在 `thread_map` 中查找现有 thread。未命中时创建新 Thread。

**触发时序**：用户消息到达 → `resolve_thread(user_id, channel, external_thread_id)` → 四元组未命中 → 锁定 session → `session.create_thread()` 构建 Thread（生成 UUID、状态 `Idle`、空 turns 列表）→ 设置 `user_id`/`channel`/`external_thread_id` → 注册到 session.threads 和 thread_map → `insert_thread()` 写入 DB。同时更新 session 的 `active_thread_id` → `update_session_active_thread()` 写入 DB。

**写入数据**：`id`、`session_id`、`user_id`、`channel`、`external_thread_id`、`state`（`Idle`）、`metadata`、`pending_approvals`（`[]`）、`created_at`、`updated_at`。

#### 3. 开始新 turn — `insert_turn()`

**触发场景**：Thread 状态为 `Idle` 且收到新的用户输入，`thread.start_turn(user_input)` 创建新的 Turn 实例并将 Thread 状态切换为 `Processing`。

**触发时序**：用户消息到达 → thread 状态检查（必须为 `Idle`）→ `start_turn(user_input)` → `Turn::new(session_id, thread_id, turn_number, user_input)` → 追加到 thread.turns → Thread 状态改为 `Processing` → `updated_at` 更新 → `insert_turn()` 写入 DB + `update_thread_state()` 更新 Thread 状态为 `Processing`。

**写入数据**：`id`、`thread_id`、`session_id`、`turn_number`、`user_input`、`state`（`Processing`）、`started_at`、其余字段为 NULL/默认值。

#### 4. 记录 tool call — `insert_tool_call()`

**触发场景**：LLM 返回的响应中包含 `tool_use` block，系统解析出工具调用请求后调用 `turn.record_tool_call(name, parameters)`。

**触发时序**：LLM 响应到达 → 解析出 `tool_use` block → `record_tool_call(name, parameters)` → 构建 `TurnToolCall { name, parameters, result: None, error: None }` → 追加到 turn.tool_calls（`call_index` = 当前 Vec 长度）→ `insert_tool_call()` 写入 DB。

**写入数据**：`turn_id`、`call_index`（0-based 序号）、`name`（工具名）、`parameters`（JSON）。`result` 和 `error` 此时为 NULL。

#### 5. 记录 tool result — `update_tool_call_result()`

**触发场景**：工具执行完成后，系统调用 `turn.record_tool_result(result)` 或 `turn.record_tool_error(error)` 回填结果到最后一个 tool call。

**触发时序**：工具执行器执行工具 → 获得结果（成功）或错误 → `record_tool_result(result)` / `record_tool_error(error)` → 更新 `tool_calls.last_mut()` 的 `result` 或 `error` 字段 → `update_tool_call_result()` 写入 DB。

**更新数据**：`result`（JSON，成功时填充）或 `error`（字符串，失败时填充），通过 `(turn_id, call_index)` 定位记录。

#### 6. 完成 turn — `complete_turn()`

**触发场景**：LLM 返回最终文本响应（不含工具调用），或工具调用循环结束后 LLM 给出最终回复。`thread.complete_turn(response)` 将 Turn 标记为 `Completed`。

**触发时序**：LLM 最终响应 → `complete_turn(response)` → `turn.complete(response)` 设置 Turn 状态为 `Completed`、记录 `response`/`completed_at` → Thread 状态改为 `Idle`、`updated_at` 更新 → `complete_turn()` 写入 DB + `update_thread_state()` 更新 Thread 状态。

**更新数据**：`SET state='Completed', response=?, thinking=?, completed_at=? WHERE id=?`。

#### 7. 失败 turn — `fail_turn()`

**触发场景**：LLM 调用抛出异常、工具执行超时、达到最大迭代次数、或工具被用户拒绝等错误情况。`thread.fail_turn(error)` 将 Turn 标记为 `Failed`。

**触发时序**：异常/超时/拒绝 → `fail_turn(error)` → `turn.fail(error)` 设置 Turn 状态为 `Failed`、记录 `error`/`completed_at` → Thread 状态改为 `Idle`、`updated_at` 更新 → `fail_turn()` 写入 DB + `update_thread_state()` 更新 Thread 状态。

**更新数据**：`SET state='Failed', error=?, completed_at=? WHERE id=?`。

#### 8. 中断 turn — `interrupt_turn()` + `update_thread_state()`

**触发场景**：Turn 正在执行中（`Processing`），用户发送新消息打断当前 turn，或系统主动中断（如进程关闭信号）。`thread.interrupt()` 同时修改 Turn 和 Thread 的状态。

**触发时序**：中断信号到达 → `thread.interrupt()` → `turn.interrupt()` 设置当前 Turn 状态为 `Interrupted`、记录 `completed_at`、清空 `image_content_parts` → Thread 状态改为 `Interrupted`、`updated_at` 更新 → `interrupt_turn()` + `update_thread_state()` 写入 DB。

**更新数据**：Turn: `SET state='Interrupted', completed_at=? WHERE id=?`；Thread: `SET state='Interrupted', updated_at=? WHERE id=?`。

#### 9. 自动审批工具 — `update_session_auto_approved_tools()`

**触发场景**：用户在工具审批提示中选择"始终允许"，`session.auto_approve_tool(tool_name)` 将工具名加入 `auto_approved_tools` HashSet。后续该工具的调用将跳过审批流程（`is_tool_auto_approved()` 返回 `true`）。

**触发时序**：工具审批请求 → 用户选择"始终允许" → `auto_approve_tool(tool_name)` → `self.auto_approved_tools.insert(tool_name)` → `update_session_auto_approved_tools()` 写入 DB。

**更新数据**：`SET auto_approved_tools=? WHERE id=?`（JSON 数组，追加新工具名）。

#### 10. Thread 状态变更 — `update_thread_state()` / `update_thread_pending_approvals()`

**触发场景**：Thread 状态在各状态之间流转时触发。覆盖上述未单独列出的状态转换，分两种情况：

- **纯状态变更**（`update_thread_state()`）：如中断后恢复（`thread.resume()` 将 `Interrupted` → `Idle`）、审批通过后恢复执行（`AwaitingApproval` → `Processing`）。仅更新 `state` 和 `updated_at`。
- **涉及审批的状态变更**（`update_thread_pending_approvals()`）：如进入审批等待状态（`AwaitingApproval`，工具需要用户确认时）、审批完成后清空审批列表。同时更新 `state`、`pending_approvals` 和 `updated_at`。

**触发时序**：状态转换函数调用 → 修改 `thread.state` + `thread.updated_at`（+ `thread.pending_approvals` 如涉及审批）→ 调用对应的特定 update 方法写入 DB。

**更新数据**：
- `update_thread_state()`: `SET state=?, updated_at=? WHERE id=?`
- `update_thread_pending_approvals()`: `SET state=?, pending_approvals=?, updated_at=? WHERE id=?`

#### 11. 清理过期 session — `delete_session()`

**触发场景**：定时任务调用 `prune_stale_sessions(max_idle)`，遍历所有 session，清理 `last_active_at` 早于截止时间（`now - max_idle`）的不活跃 session。

**触发时序**：定时触发 → 读取所有 session → 计算 `cutoff = now - max_idle` → 筛选 `last_active_at < cutoff` 的 session → 从 `self.sessions` HashMap 中移除 → 清理 `self.thread_map` 中所有 `binding_id` 匹配的条目 → `delete_session()` 写入 DB → 级联删除关联的 threads、turns、tool_calls（依赖 `ON DELETE CASCADE`）。

**删除数据**：目标 session 行及其所有关联数据（threads → turns → turn_tool_calls 级联删除）。

---

## 7. 上下文恢复验证

重启后 `Thread.messages()` 必须能正确重建 LLM 上下文序列：

```
user(input)
→ assistant_with_tool_calls([{name, args}])
→ tool_result(call_id, name, content)
→ assistant(response)
```

验证链路：
1. `load_all_sessions()` → 加载 sessions + threads + turns + tool_calls
2. 每个 Turn 的 `tool_calls: Vec<TurnToolCall>` 从 `turn_tool_calls` 表按 `call_index` 排序还原
3. `Thread.messages()` 遍历 turns 构建 `Vec<ChatMessage>`
4. 生成的消息序列应与重启前完全一致（忽略 `image_content_parts`）

---

## 8. Diesel 迁移策略

### 8.1 项目配置

`diesel.toml`（项目根目录）:

```toml
[print_schema]
file = "src/storage/schema.rs"

[migrations_directory]
dir = "migrations"
```

### 8.2 创建迁移

```bash
diesel setup --database-url=workspace/xcraw.db

# 单个迁移包含所有表（原子性）
diesel migration generate create_session_tables
```

`up.sql` 包含全部 4 张表的 CREATE TABLE + 索引。
`down.sql` 按反向依赖顺序 DROP TABLE。

### 8.3 运行时自动迁移

`Database::new()` 内部调用 `diesel_migrations::run_pending_migrations()`，确保应用启动时 schema 与代码一致。

---

## 9. 配置

可在 YAML 配置中自定义数据库路径：

```yaml
storage:
  database_url: "workspace/storage/xcraw.db"   # 相对于工作目录
```

不配置时默认使用 `workspace/storage/xcraw.db`。

---

## 10. 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `diesel.toml` | 新建 | Diesel ORM 配置 |
| `migrations/XXXX_create_session_tables/up.sql` | 新建 | 4 张表 + 索引 |
| `migrations/XXXX_create_session_tables/down.sql` | 新建 | DROP TABLE |
| `src/storage/mod.rs` | 重写 | Database + 连接池 + 公开 API |
| `src/storage/models.rs` | 新建 | Diesel 模型结构体 |
| `src/storage/schema.rs` | 生成 | `diesel print-schema` 自动生成 |
| `src/storage/convert.rs` | 新建 | 内存模型 ↔ DB 模型转换 |
| `src/session/manager.rs` | 修改 | 添加 `db` 字段 + `new_with_db()` + 各写入点 |
| `src/config.rs` | 修改 | 添加 `storage.database_url` 配置 |
| `src/main.rs` | 修改 | 初始化 Database，传给 SessionManager |
| `Cargo.toml` | 修改 | 添加 diesel `r2d2` feature |

---

## 11. 相关文档

- [Session 模块](session.md) — Session/Thread/Turn 内存模型
- [Binding Loop](binding-loop.md) — 消息处理循环与状态转换点
- [配置系统](config.md) — YAML 配置结构