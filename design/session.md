# Session 模块

## 1. 概述

Session 模块负责管理 **binding 级运行时上下文**，而不是单纯保存聊天记录。

在新的模型里：

- **一个 Session 对应一个 binding**
- **一个 Session 拥有多个 Thread**
- **一个 Thread 拥有多个 Turn**
- **一个 Turn 表示一轮完整 agentic run**：`question -> thinking -> tool_call -> answer`

因此，`Session` 的意义是：**binding 的运行时上下文容器，而不是聊天记录本身**。

它解决的是三件事：

1. **归属边界**：这些 threads 属于哪个 binding
2. **共享状态**：哪些状态应在同一 binding 下的多个 threads 之间共享
3. **生命周期容器**：binding 存活期间，session 负责承载和组织所有 thread

---

## 2. 核心概念

### 2.1 分层模型

```
Binding
└── Session                   // binding 级运行时容器
    ├── active_thread_id
    ├── auto_approved_tools
    ├── metadata
    └── threads: HashMap<ThreadId, Thread>
        └── Thread            // 一条连续对话流
            ├── user_id
            ├── channel
            ├── external_thread_id
            ├── state
            ├── pending_approval
            └── turns: Vec<Turn>
                └── Turn      // 一轮完整交互
                    ├── user_input
                    ├── thinking
                    ├── tool_calls
                    ├── response
                    ├── state
                    └── timestamps
```

### 2.2 各层职责

#### Binding

Binding 是外部世界入口，负责：

- 接收 channel 消息
- 发送 channel 响应
- 调用 `SessionManager`
- 驱动 agent loop

Binding 本身不承载具体会话历史。

#### Session

Session 是 binding 的运行时上下文容器，负责：

- 保存该 binding 下的所有 threads
- 保存跨 thread 共享的状态
- 维护当前活跃 thread
- 维护 session 级偏好，例如 `auto_approved_tools`

Session 不直接表示某一条具体对话。

#### Thread

Thread 是一条连续对话流，负责：

- 对应外部 channel 中的一条 conversation / thread
- 承载 turn 列表
- 维护处理中、审批中、中断、恢复等生命周期状态
- 作为 LLM 历史上下文重建的单位

#### Turn

Turn 是一次完整 agentic run，负责记录：

- 用户输入
- thinking / reasoning
- tool call 与 tool result
- 最终回答
- 本轮执行状态和时间戳

一个 turn 从用户输入开始，到最终 answer 或失败结束。

---

## 3. 设计判断标准

为了避免字段放错层级，统一使用以下判断标准：

- **只影响单条对话流** → 放 `Thread`
- **影响同一 binding 下多条 thread** → 放 `Session`
- **只影响一轮 agentic run** → 放 `Turn`

也可以这样理解：

- `Thread` 回答：**现在在处理哪条对话？**
- `Turn` 回答：**这一轮具体做了什么？**
- `Session` 回答：**这些对话属于谁，以及共享什么状态？**

---

## 4. 数据结构建议

### 4.1 Session

```rust
pub struct Session {
    /// 唯一 ID
    pub id: Uuid,
    /// 该 session 所属 binding
    pub binding_id: String,
    /// 当前活跃 thread
    pub active_thread: Option<Uuid>,
    /// 该 binding 下的全部 threads
    pub threads: HashMap<Uuid, Thread>,
    /// session 级共享偏好，例如 always approve 的工具
    pub auto_approved_tools: HashSet<String>,
    /// 额外元数据
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}
```

### 4.2 Thread

```rust
pub enum ThreadState {
    Idle,
    Processing,
    AwaitingApproval,
    Completed,
    Interrupted,
}

pub struct Thread {
    pub id: Uuid,
    pub session_id: Uuid,

    /// 外部路由身份
    pub user_id: String,
    pub channel: String,
    pub external_thread_id: Option<String>,

    /// 生命周期状态
    pub state: ThreadState,
    /// 当前待审批请求
    pub pending_approval: Option<PendingApproval>,
    /// 该 thread 下的所有 turns
    pub turns: Vec<Turn>,

    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### 4.3 Turn

```rust
pub enum TurnState {
    Running,
    Completed,
    Failed,
    Interrupted,
}

pub struct Turn {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub turn_number: usize,

    pub user_input: String,
    pub thinking: Option<String>,
    pub tool_calls: Vec<ToolExecution>,
    pub response: Option<String>,
    pub state: TurnState,

    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

---

## 5. 路由模型

### 5.1 ThreadKey

Thread 的查找不应该再只依赖 chat room，而应该依赖外部 conversation 身份。

推荐使用：

```rust
struct ThreadKey {
    user_id: String,
    channel: String,
    external_thread_id: Option<String>,
}
```

### 5.2 路由语义

- 相同 `(user_id, channel, external_thread_id)` → 命中同一个 internal thread
- 不同 `external_thread_id` → 命中不同 internal thread
- 相同用户但不同 channel → 命中不同 internal thread
- `None` 与 `Some(...)` 必须视为不同 key

这意味着：

- 在非 threaded 平台/场景中，可以稳定复用 `(user, channel, None)` 对应的 thread
- 在 threaded 平台中，不同外部 thread 会映射到不同 internal thread

---

## 6. 生命周期

### 6.1 Session 生命周期

- binding 启动时，创建或获取其 session
- binding 存活期间，session 一直存在
- 新 thread 创建、切换、完成，都发生在该 session 内
- session 负责保存跨 thread 共享状态

### 6.2 Thread 生命周期

典型状态流转：

```text
Idle -> Processing -> AwaitingApproval -> Processing -> Idle
Idle -> Processing -> Interrupted -> Idle
Idle -> Processing -> Completed
```

说明：

- `Idle`：等待新的输入
- `Processing`：正在执行当前 turn
- `AwaitingApproval`：当前 turn 因工具审批暂停
- `Interrupted`：当前 turn 被中断，可恢复或重启
- `Completed`：thread 生命周期结束，不再接收新 turn

### 6.3 Turn 生命周期

一个 turn 包含：

```text
user_input
  -> thinking
  -> tool_call (0..n)
  -> tool_result (0..n)
  -> final_response
```

重要约束：

- 工具审批暂停的是 **当前 turn**，不是整个 session
- 审批回复恢复的是 **原 thread 的原 turn**
- 审批流程不应创建新的 turn

---

## 7. 审批模型

推荐将审批状态挂在 `Thread` 上，而不是 `Session` 上。

原因：

- 审批恢复的是某条具体 thread 的执行上下文
- 一个 thread 在任意时刻通常只会有一个活动审批点
- session 级只负责保存“always approve”这种共享偏好

推荐模型：

```rust
pub struct PendingApproval {
    pub request_id: Uuid,
    pub tool_name: String,
    pub parameters: serde_json::Value,
    pub display_parameters: serde_json::Value,
    pub description: String,
    pub tool_call_id: String,
    pub context_messages: Vec<ChatMessage>,
    pub user_timezone: Option<String>,
}
```

约束：

- `ThreadState::AwaitingApproval` <=> `pending_approval.is_some()`
- 审批通过后恢复原 turn
- 审批拒绝后，当前 turn 可失败或写入 tool error 后回到 idle

---

## 8. SessionManager

### 8.1 核心职责

`SessionManager` 负责：

- 获取/创建 binding 对应的 session
- 将外部消息解析到正确的 thread
- 在需要时创建新的 thread
- 维护 `ThreadKey -> ThreadId` 映射

### 8.2 推荐结构

```rust
pub struct SessionManager {
    /// binding 级 session
    sessions: HashMap<String, Arc<Mutex<Session>>>,
    /// 外部 conversation 到 internal thread 的映射
    thread_map: HashMap<ThreadKey, Uuid>,
}
```

### 8.3 核心接口

```rust
impl SessionManager {
    async fn get_or_create_session(&self, binding_id: &str) -> Arc<Mutex<Session>>{}

    async fn resolve_thread(
        &self,
        binding_id: &str,
        user_id: &str,
        channel: &str,
        external_thread_id: Option<&str>,
    ) -> (Arc<Mutex<Session>>, Uuid){}

    async fn register_thread(
        &self,
        binding_id: &str,
        user_id: &str,
        channel: &str,
        external_thread_id: Option<&str>,
        thread_id: Uuid,
    ){}
}
```

---

## 9. 与 Binding 集成

`Binding::handle_message()` 是消息生命周期入口。

推荐流程：

```text
IncomingMessage
  -> parse intent
  -> resolve session
  -> resolve thread
  -> dispatch by thread state
  -> start/resume/approval/complete turn
  -> send response back to channel
```

### 9.1 普通用户输入

- 根据 `(user_id, channel, external_thread_id)` 定位 thread
- 若 thread 不存在，则创建新 thread
- 在该 thread 上调用 `start_turn(user_input)`
- 进入 loop 执行 reasoning / tool calls / answer

### 9.2 审批回复

- 必须定位到原 thread
- 不创建新 turn
- 读取 `pending_approval`
- 审批通过：恢复当前 turn
- 审批拒绝：结束当前审批点，并将 turn 标记失败或记录工具错误

### 9.3 自动审批

若用户选择 “always approve”，则将工具名写入：

```
session.auto_approved_tools
```

这是 session 级共享偏好，因此应保存在 session，而不是某个单独 turn 上。

---

## 10. 与 LLM 上下文集成

LLM 的历史上下文建议以 `Thread` 为单位构建，而不是以整个 session 为单位直接拼接。

推荐复用 thread 上的方法：

- `Thread.messages()`：将 turn 历史还原为 LLM 可消费的消息序列
- `Thread.restore_from_messages()`：从历史消息恢复 thread 状态

上下文序列建议保持：

```text
user
-> assistant(tool_calls)
-> tool_result
-> assistant
```

这样可以避免模型重复调用已经完成过的工具。

---

## 11. 与旧设计的区别

旧设计把 Session 理解为：

- 一个 Chat Room 下的活跃/归档消息集合
- `/reset` 会创建新 session，旧 session 归档
- 上下文主要通过“最近 N 条消息”构建

新设计将 Session 重新定义为：

- binding 的运行时上下文容器
- 其核心内容是多个 threads，而不是单一 messages 列表
- 对话连续性的主单位是 `Thread`
- 上下文构建的主单位也是 `Thread`
- `Turn` 成为 tool calling 与审批恢复的最小执行单位

如果未来仍需要“reset / archive / history”能力，建议作为 **session persistence / archival policy** 的扩展能力处理，而不是作为 Session 的核心定义。

---

## 12. 设计要点

1. **Session 是容器，不是消息本身**
2. **Thread 是连续对话流的主单位**
3. **Turn 是一次完整 agentic run 的主单位**
4. **审批状态属于 thread，不属于 session**
5. **自动审批偏好属于 session，不属于 turn**
6. **路由必须基于 `(user_id, channel, external_thread_id)`**
7. **LLM 上下文应按 thread 构建，而不是无差别拼接整个 session**
8. **binding / session / thread / turn 的职责必须严格分层**

---

## 13. 相关文档

- [Chat Room 系统](chat-room.md) - Chat Room 与 Binding/Session 的关系
- [Agent 定义系统](agent-definition.md) - Agent 如何在 loop 中使用 thread 上下文
- [任务系统](task-system.md) - task 与 session/thread 的关系
