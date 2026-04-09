# xclaw Hook System 设计文档

## 1. 概述

Hook 系统是 xclaw 的核心扩展机制，允许开发者在关键事件点插入自定义逻辑，实现日志记录、指标收集、消息拦截、工具调用监控等功能。

### 设计原则

- **Trait-based**: 使用 Rust trait 定义 Hook 接口，类型安全
- **异步优先**: 所有 Hook 方法均为 async，支持异步操作
- **优先级排序**: 支持按优先级控制 Hook 执行顺序
- **配置驱动**: 通过配置文件启用/禁用 Hook
- **可拦截**: MessageHook 和 ToolHook 支持拦截和修改

## 2. Hook 类型

### 2.1 TaskHook - 任务生命周期

```rust
pub trait TaskHook: Hook {
    fn on_task_created(&self, ctx: &HookContext, task: &Task) -> HookFuture<HookResult>;
    fn on_task_claimed(&self, ctx: &HookContext, task: &Task) -> HookFuture<HookResult>;
    fn on_task_progress(&self, ctx: &HookContext, task: &Task) -> HookFuture<HookResult>;
    fn on_task_completed(&self, ctx: &HookContext, task: &Task) -> HookFuture<HookResult>;
    fn on_task_failed(&self, ctx: &HookContext, task: &Task, error: &TaskError) -> HookFuture<HookResult>;
    fn on_task_cancelled(&self, ctx: &HookContext, task: &Task) -> HookFuture<HookResult>;
    fn on_task_stalled(&self, ctx: &HookContext, task: &Task) -> HookFuture<HookResult>;
}
```

**触发时机**:
| 方法 | 触发时机 | 典型用途 |
|------|----------|----------|
| `on_task_created` | 调用 `task_create` 工具后 | 通知、审计日志 |
| `on_task_claimed` | Agent 认领任务后 | 更新任务看板 |
| `on_task_progress` | 调用 `task_progress` 后 | 进度监控 |
| `on_task_completed` | 任务标记完成后 | 结果归档、通知 |
| `on_task_failed` | 任务执行失败 | 错误报告 |
| `on_task_cancelled` | 任务被取消 | 资源清理 |
| `on_task_stalled` | 心跳检测到停滞 | 告警 |

### 2.2 AgentHook - Agent 生命周期

```rust
pub trait AgentHook: Hook {
    fn on_agent_joined(&self, ctx: &HookContext, agent_id: &str) -> HookFuture<HookResult>;
    fn on_agent_left(&self, ctx: &HookContext, agent_id: &str) -> HookFuture<HookResult>;
    fn on_agent_heartbeat(&self, ctx: &HookContext, agent_id: &str) -> HookFuture<HookResult>;
    fn on_agent_unresponsive(&self, ctx: &HookContext, agent_id: &str) -> HookFuture<HookResult>;
}
```

### 2.3 MessageHook - 消息事件

```rust
pub trait MessageHook: Hook {
    fn on_message_received(&self, ctx: &HookContext, msg: &RoomMessage) -> HookFuture<MessageHookResult>;
    fn on_message_sent(&self, ctx: &HookContext, msg: &RoomMessage) -> HookFuture<HookResult>;
    fn on_mention_received(&self, ctx: &HookContext, msg: &RoomMessage) -> HookFuture<HookResult>;
}
```

**特点**: `on_message_received` 返回 `MessageHookResult`，支持:
- `Continue` - 继续处理消息
- `Block` - 阻止消息（如垃圾过滤）
- `Modify(RoomMessage)` - 修改后转发

### 2.4 ToolHook - 工具调用

```rust
pub trait ToolHook: Hook {
    fn before_tool_call(&self, ctx: &HookContext, tool_name: &str, params: &Value) -> HookFuture<ToolHookResult>;
    fn after_tool_call(&self, ctx: &HookContext, tool_name: &str, result: &Value) -> HookFuture<HookResult>;
    fn on_tool_error(&self, ctx: &HookContext, tool_name: &str, error: &ToolError) -> HookFuture<HookResult>;
}
```

**特点**: `before_tool_call` 返回 `ToolHookResult`，支持:
- `Continue` - 继续执行工具
- `Block(String)` - 阻止执行并返回错误信息
- `Modify { name, params }` - 修改工具名或参数后执行

## 3. 核心类型

### 3.1 HookContext

传递给所有 Hook 的上下文信息：

```rust
pub struct HookContext {
    /// 触发事件的 Agent ID
    pub agent_id: Option<String>,
    /// 关联的任务 ID
    pub task_id: Option<TaskId>,
    /// 通信渠道（Discord/Slack/Matrix）
    pub channel: Option<String>,
    /// 事件发生时间
    pub timestamp: DateTime<Utc>,
    /// 扩展元数据
    pub metadata: HashMap<String, serde_json::Value>,
}
```

**Builder 用法**:
```rust
let ctx = HookContext::builder()
    .with_agent("agent-a")
    .with_task(task_id)
    .with_channel("discord")
    .with_metadata("source", "heartbeat")
    .build();
```

### 3.2 HookResult

```rust
pub enum HookResult {
    Ok,
    Err(HookError),
}

pub enum MessageHookResult {
    Continue,
    Block,
    Modify(RoomMessage),
}

pub enum ToolHookResult {
    Continue,
    Block(String),
    Modify { name: String, params: Value },
}
```

### 3.3 基础 Hook Trait

```rust
pub trait Hook: Send + Sync {
    /// Hook 名称（用于日志和配置）
    fn name(&self) -> &str;

    /// 优先级 - 数字越小越先执行（默认: 100）
    fn priority(&self) -> i32 {
        100
    }

    /// 是否启用（默认: true）
    fn enabled(&self) -> bool {
        true
    }
}
```

## 4. HookRegistry

### 4.1 API 设计

```rust
pub struct HookRegistry {
    task_hooks: Vec<Box<dyn TaskHook>>,
    agent_hooks: Vec<Box<dyn AgentHook>>,
    message_hooks: Vec<Box<dyn MessageHook>>,
    tool_hooks: Vec<Box<dyn ToolHook>>,
    config: HookConfig,
}

impl HookRegistry {
    /// 创建空 Registry
    pub fn new() -> Self;

    /// 从配置创建
    pub fn from_config(config: HookConfig) -> Self;

    /// 注册 Hook
    pub fn register_task_hook(&mut self, hook: Box<dyn TaskHook>);
    pub fn register_agent_hook(&mut self, hook: Box<dyn AgentHook>);
    pub fn register_message_hook(&mut self, hook: Box<dyn MessageHook>);
    pub fn register_tool_hook(&mut self, hook: Box<dyn ToolHook>);

    /// 启用/禁用 Hook
    pub fn enable_hook(&mut self, name: &str);
    pub fn disable_hook(&mut self, name: &str);

    /// 执行 Hook 链
    pub async fn on_task_created(&self, ctx: &HookContext, task: &Task) -> HookResult;
    pub async fn on_message_received(&self, ctx: &HookContext, msg: &RoomMessage) -> MessageHookResult;
    pub async fn before_tool_call(&self, ctx: &HookContext, tool: &str, params: &Value) -> ToolHookResult;
    // ... 其他执行方法
}
```

### 4.2 执行流程

```
1. 系统事件触发
   │
   ▼
2. HookRegistry 接收事件
   │
   ├─▶ 获取对应类型的所有 Hook
   ├─▶ 按 priority 升序排序
   ├─▶ 过滤掉 !enabled() 的 Hook
   │
   ▼
3. 顺序执行每个 Hook
   │
   ├─▶ 调用 hook.method(ctx, data).await
   ├─▶ 如果返回 Err → Fail-fast，立即返回错误
   ├─▶ 如果返回 Block → 立即返回 Block
   └─▶ 继续下一个 Hook
   │
   ▼
4. 返回最终结果
```

## 5. 配置

### 5.1 CONFIG.yaml 配置

```yaml
hooks:
  # 全局开关
  enabled: true

  # 各 Hook 的启用状态（覆盖代码中的 enabled()）
  logging:
    enabled: true
    priority: 10  # 可选：覆盖代码中的 priority()

  metrics:
    enabled: true
    priority: 20

  spam_filter:
    enabled: false
```

### 5.2 HookConfig 结构

```rust
pub struct HookConfig {
    pub enabled: bool,
    pub overrides: HashMap<String, HookOverride>,
}

pub struct HookOverride {
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}
```

## 6. 内置 Hook 示例

### 6.1 日志 Hook

```rust
pub struct LoggingHook;

impl Hook for LoggingHook {
    fn name(&self) -> &str { "logging" }
    fn priority(&self) -> i32 { 10 }  // 优先执行
}

impl TaskHook for LoggingHook {
    async fn on_task_created(&self, ctx: &HookContext, task: &Task) -> HookResult {
        info!(
            agent = ctx.agent_id,
            task = task.short_id,
            "Task created: {}", task.title
        );
        HookResult::Ok
    }
    // ... 其他方法
}
```

### 6.2 消息过滤 Hook

```rust
pub struct SpamFilterHook {
    blocked_patterns: Vec<Regex>,
}

impl MessageHook for SpamFilterHook {
    async fn on_message_received(&self, _ctx: &HookContext, msg: &RoomMessage) -> MessageHookResult {
        for pattern in &self.blocked_patterns {
            if pattern.is_match(&msg.content) {
                warn!("Blocked spam message from {}", msg.sender);
                return MessageHookResult::Block;
            }
        }
        MessageHookResult::Continue
    }
}
```

## 7. 集成点

### 7.1 TaskStore

```rust
impl TaskStore {
    pub async fn create(&self, task: Task) -> Result<TaskId> {
        // 1. 保存任务
        self.save(&task).await?;

        // 2. 触发 Hook
        let ctx = HookContext::builder()
            .with_agent(&task.created_by)
            .with_task(task.task_id)
            .build();

        if let Err(e) = self.hooks.on_task_created(&ctx, &task).await {
            error!("Task created hook failed: {}", e);
            // 根据策略决定是否回滚
        }

        Ok(task.task_id)
    }
}
```

### 7.2 ChatRoom

```rust
async fn handle_message(&self, msg: Message) {
    let room_msg = RoomMessage::from(msg);

    // 触发接收 Hook
    let ctx = HookContext::builder()
        .with_channel("discord")
        .build();

    match self.hooks.on_message_received(&ctx, &room_msg).await {
        MessageHookResult::Block => return,  // 阻止处理
        MessageHookResult::Modify(modified) => {
            // 使用修改后的消息继续
            self.process_message(modified).await;
        }
        MessageHookResult::Continue => {
            self.process_message(room_msg).await;
        }
    }
}
```

### 7.3 Tool 执行

```rust
async fn execute_tool(&self, name: &str, params: Value) -> Result<Value> {
    let ctx = HookContext::builder()
        .with_agent(self.agent_id())
        .build();

    // 前置 Hook
    let (name, params) = match self.hooks.before_tool_call(&ctx, name, &params).await {
        ToolHookResult::Block(reason) => {
            return Err(Error::BlockedByHook(reason));
        }
        ToolHookResult::Modify { name, params } => (name, params),
        ToolHookResult::Continue => (name.to_string(), params),
    };

    // 执行工具
    let result = self.tool_executor.call(&name, params).await;

    // 后置 Hook
    match &result {
        Ok(value) => {
            self.hooks.after_tool_call(&ctx, &name, value).await;
        }
        Err(e) => {
            self.hooks.on_tool_error(&ctx, &name, &e.into()).await;
        }
    }

    result
}
```

## 8. 目录结构

```
src/hooks/
├── mod.rs          # 模块导出和公共接口
├── trait.rs        # Hook trait 定义
├── types.rs        # HookResult, MessageHookResult, ToolHookResult, Task, TaskError, RoomMessage
├── context.rs      # HookContext
├── registry.rs     # HookRegistry
├── config.rs       # HookConfig
└── builtin/        # 内置 Hook
    ├── mod.rs
    ├── logging.rs
    └── metrics.rs
```

## 9. 使用示例

### 9.1 创建自定义 Hook

```rust
use xclaw::hooks::*;

pub struct AuditHook {
    audit_log: Arc<Mutex<Vec<AuditEntry>>>,
}

impl Hook for AuditHook {
    fn name(&self) -> &str { "audit" }
    fn priority(&self) -> i32 { 50 }
}

impl TaskHook for AuditHook {
    async fn on_task_created(&self, ctx: &HookContext, task: &Task) -> HookResult {
        let entry = AuditEntry {
            timestamp: ctx.timestamp,
            action: "task_created",
            agent: ctx.agent_id.clone(),
            task: task.short_id.clone(),
        };
        self.audit_log.lock().await.push(entry);
        HookResult::Ok
    }
    // ...
}
```

### 9.2 注册 Hook

```rust
// main.rs
use xclaw::hooks::*;

#[tokio::main]
async fn main() {
    let config = HookConfig::load("workspace/CONFIG.yaml");
    let mut registry = HookRegistry::from_config(config);

    // 注册内置 Hook
    registry.register_task_hook(Box::new(LoggingHook));
    registry.register_task_hook(Box::new(MetricsHook));

    // 注册自定义 Hook
    registry.register_task_hook(Box::new(AuditHook::new()));
    registry.register_message_hook(Box::new(SpamFilterHook::new()));

    // 创建服务时传入 registry
    let service = AgentService::new(registry);
    service.run().await;
}
```

## 10. 注意事项

1. **Fail-fast 策略**: 当一个 Hook 返回错误时，后续 Hook 不再执行
2. **执行顺序**: 按 priority 升序执行，相同 priority 按注册顺序
3. **异步安全**: Hook 内部需要处理好自己的并发安全
4. **避免递归**: Hook 中触发的事件不应导致无限递归
5. **性能考虑**: Hook 执行是顺序的，避免耗时操作
