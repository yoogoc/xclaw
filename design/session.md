# Session 模块

## 1. 概述

Session 模块负责存储和维护对话历史。每个 Chat Room 可以有多个 Session，用户可以通过发送 `/reset` 命令重置 Session（创建新的 Session，旧的 Session 归档保留）。

**核心特性**:
- 每个 Chat Room 维护多个 Session（历史归档 + 当前活跃）
- `/reset` 命令创建新 Session，旧 Session 保持为历史
- Session 持久化存储在文件系统
- 支持按 Session 检索历史对话
- 支持上下文窗口控制（加载最近 N 条消息到 LLM 上下文）

## 2. 核心概念

### 2.1 Session 定义

```rust
/// Session 唯一标识符
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    /// 短格式用于显示
    pub fn short(&self) -> String {
        self.0.to_string()[..8].to_string()
    }
}

/// Session 状态
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SessionStatus {
    /// 当前活跃，接收新消息
    Active,
    /// 已归档（被 reset 后）
    Archived,
}

/// Session 数据结构
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub session_id: SessionId,
    pub chat_room_id: String,       // 所属 Chat Room
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,  // 归档时间
    pub status: SessionStatus,
    pub message_count: usize,
    pub metadata: SessionMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// 创建者信息
    pub created_by: String,
    /// Session 主题/摘要（由 LLM 自动生成）
    pub summary: Option<String>,
    /// 关联的任务列表
    pub related_tasks: Vec<String>,
    /// 自定义标签
    pub tags: Vec<String>,
}
```

### 2.2 Message 定义

```rust
/// 消息唯一标识符
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageId(pub Uuid);

/// 消息发送者
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Sender {
    User { id: String, name: String },
    Agent(String),  // agent_id
    System,         // 系统消息
}

/// 消息类型
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MessageType {
    Text,
    ToolCall { tool_name: String, parameters: Value },
    ToolResult { tool_name: String, result: Value },
    Command { command: String },  // /reset, /help 等
}

/// 消息数据结构
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub timestamp: DateTime<Utc>,
    pub sender: Sender,
    pub content: String,
    pub message_type: MessageType,
    /// 关联的 task_id（如果消息与任务相关）
    pub task_ids: Vec<String>,
    /// 回复的消息 ID（支持线程）
    pub reply_to: Option<MessageId>,
}
```

## 3. 存储结构

### 3.1 目录布局

```
workspace/
└── sessions/
    ├── index.json                    # Session 索引（快速查找）
    └── {chat_room_id}/
        ├── active.json               # 软链接：指向当前活跃 session
        ├── metadata.json             # Chat Room 的 session 元数据
        └── {session_id}.json         # 单个 session 的消息记录
            
            # 示例：
            sessions/
            ├── index.json
            └── discord-main/
                ├── active.json -> 20240115-140000-abc123.json
                ├── metadata.json
                ├── 20240115-090000-xxx111.json  # 已归档
                ├── 20240115-103000-yyy222.json  # 已归档
                └── 20240115-140000-abc123.json  # 当前活跃
```

### 3.2 Index 文件结构

```json
{
  "version": 1,
  "last_updated": "2024-01-15T14:00:00Z",
  "chat_rooms": {
    "discord-main": {
      "active_session": "20240115-140000-abc123",
      "session_count": 3,
      "sessions": [
        {
          "session_id": "20240115-140000-abc123",
          "status": "active",
          "created_at": "2024-01-15T14:00:00Z",
          "message_count": 15,
          "summary": "代码库分析和任务分配"
        },
        {
          "session_id": "20240115-103000-yyy222",
          "status": "archived",
          "created_at": "2024-01-15T10:30:00Z",
          "archived_at": "2024-01-15T14:00:00Z",
          "message_count": 42,
          "summary": "项目初始化和需求讨论"
        }
      ]
    }
  }
}
```

### 3.3 Session 文件结构

```json
{
  "session_id": "20240115-140000-abc123",
  "chat_room_id": "discord-main",
  "created_at": "2024-01-15T14:00:00Z",
  "updated_at": "2024-01-15T14:30:00Z",
  "archived_at": null,
  "status": "active",
  "message_count": 15,
  "metadata": {
    "created_by": "user#1234",
    "summary": "代码库分析和任务分配",
    "related_tasks": ["a7b3c9d2", "f8e2d1a3"],
    "tags": ["code-analysis", "onboarding"]
  },
  "messages": [
    {
      "message_id": "msg-001",
      "session_id": "20240115-140000-abc123",
      "timestamp": "2024-01-15T14:00:00Z",
      "sender": { "type": "user", "id": "user#1234", "name": "张三" },
      "content": "帮我分析这个代码库",
      "message_type": "text",
      "task_ids": [],
      "reply_to": null
    },
    {
      "message_id": "msg-002",
      "session_id": "20240115-140000-abc123",
      "timestamp": "2024-01-15T14:00:05Z",
      "sender": { "type": "agent", "agent_id": "agent-a" },
      "content": "好的，我来分析代码库结构...",
      "message_type": "text",
      "task_ids": ["a7b3c9d2", "f8e2d1a3"],
      "reply_to": "msg-001"
    }
  ]
}
```

## 4. SessionManager 实现

### 4.1 核心结构

```rust
pub struct SessionManager {
    workspace_path: PathBuf,
    /// 内存缓存（可选）
    cache: Arc<RwLock<SessionCache>>,
}

pub struct SessionCache {
    /// active_sessions: chat_room_id -> Session
    active_sessions: HashMap<String, Session>,
    /// recent_messages: session_id -> Vec<Message>
    recent_messages: HashMap<SessionId, Vec<Message>>,
}
```

### 4.2 核心接口

```rust
impl SessionManager {
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            workspace_path: workspace_path.join("sessions"),
            cache: Arc::new(RwLock::new(SessionCache::new())),
        }
    }
    
    /// 获取或创建 Chat Room 的活跃 Session
    pub async fn get_or_create_active(
        &self,
        chat_room_id: &str,
        created_by: &str,
    ) -> Result<Session> {
        // 1. 检查内存缓存
        if let Some(session) = self.cache.read().active_sessions.get(chat_room_id) {
            return Ok(session.clone());
        }
        
        // 2. 检查文件系统
        let index = self.load_index().await?;
        if let Some(room_data) = index.chat_rooms.get(chat_room_id) {
            if let Some(active_id) = &room_data.active_session {
                let session = self.load_session(chat_room_id, active_id).await?;
                // 更新缓存
                self.cache.write().active_sessions.insert(
                    chat_room_id.to_string(),
                    session.clone()
                );
                return Ok(session);
            }
        }
        
        // 3. 没有活跃 Session，创建新的
        self.create_session(chat_room_id, created_by).await
    }
    
    /// 创建新 Session
    pub async fn create_session(
        &self,
        chat_room_id: &str,
        created_by: &str,
    ) -> Result<Session> {
        let session_id = SessionId::new();
        let now = Utc::now();
        
        let session = Session {
            session_id,
            chat_room_id: chat_room_id.to_string(),
            created_at: now,
            updated_at: now,
            archived_at: None,
            status: SessionStatus::Active,
            message_count: 0,
            metadata: SessionMetadata {
                created_by: created_by.to_string(),
                summary: None,
                related_tasks: vec![],
                tags: vec![],
            },
        };
        
        // 确保目录存在
        let room_dir = self.workspace_path.join(chat_room_id);
        fs::create_dir_all(&room_dir).await?;
        
        // 保存 session 文件
        let session_path = room_dir.join(format!("{}.json", session_id.to_string()));
        fs::write(&session_path, serde_json::to_string_pretty(&session)?).await?;
        
        // 更新索引
        self.update_index(chat_room_id, &session).await?;
        
        // 更新 active.json 软链接/引用
        let active_path = room_dir.join("active.json");
        let active_ref = json!({
            "active_session_id": session_id.to_string(),
            "updated_at": now
        });
        fs::write(&active_path, serde_json::to_string_pretty(&active_ref)?).await?;
        
        // 更新缓存
        self.cache.write().active_sessions.insert(
            chat_room_id.to_string(),
            session.clone()
        );
        
        Ok(session)
    }
    
    /// 重置 Session（创建新的，归档旧的）
    pub async fn reset_session(
        &self,
        chat_room_id: &str,
        reset_by: &str,
    ) -> Result<Session> {
        // 1. 归档当前活跃 Session
        if let Some(old_session) = self.get_active_session(chat_room_id).await? {
            self.archive_session(&old_session).await?;
        }
        
        // 2. 创建新 Session
        let new_session = self.create_session(chat_room_id, reset_by).await?;
        
        info!("Session reset: {} -> {} in {}", 
            old_session.map(|s| s.session_id.short()).unwrap_or_default(),
            new_session.session_id.short(),
            chat_room_id
        );
        
        Ok(new_session)
    }
    
    /// 添加消息到当前 Session
    pub async fn add_message(
        &self,
        chat_room_id: &str,
        message: Message,
    ) -> Result<()> {
        // 1. 获取活跃 Session
        let session = self.get_or_create_active(chat_room_id, "system").await?;
        
        // 2. 追加到 session 文件
        let room_dir = self.workspace_path.join(chat_room_id);
        let session_path = room_dir.join(format!("{}.json", session.session_id.to_string()));
        
        // 读取现有内容
        let content = fs::read_to_string(&session_path).await?;
        let mut session_data: serde_json::Value = serde_json::from_str(&content)?;
        
        // 追加消息
        let messages = session_data["messages"].as_array_mut()
            .expect("messages field should be array");
        messages.push(serde_json::to_value(&message)?);
        
        // 更新元数据
        session_data["message_count"] = json!(messages.len());
        session_data["updated_at"] = json!(Utc::now());
        
        // 写回文件
        fs::write(&session_path, serde_json::to_string_pretty(&session_data)?).await?;
        
        // 3. 更新索引
        self.update_index_message_count(chat_room_id, &session.session_id, messages.len()).await?;
        
        // 4. 更新缓存
        self.cache.write().add_message(&session.session_id, message);
        
        Ok(())
    }
    
    /// 获取 Session 的所有消息
    pub async fn get_messages(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<Message>> {
        // 1. 检查缓存
        if let Some(messages) = self.cache.read().recent_messages.get(session_id) {
            return Ok(messages.clone());
        }
        
        // 2. 从文件加载
        let session = self.find_session_by_id(session_id).await?;
        let room_dir = self.workspace_path.join(&session.chat_room_id);
        let session_path = room_dir.join(format!("{}.json", session_id.to_string()));
        
        let content = fs::read_to_string(&session_path).await?;
        let session_data: serde_json::Value = serde_json::from_str(&content)?;
        
        let messages: Vec<Message> = serde_json::from_value(
            session_data["messages"].clone()
        )?;
        
        Ok(messages)
    }
    
    /// 获取最近 N 条消息（用于构建 LLM 上下文）
    pub async fn get_recent_messages(
        &self,
        chat_room_id: &str,
        limit: usize,
    ) -> Result<Vec<Message>> {
        let session = self.get_or_create_active(chat_room_id, "system").await?;
        let messages = self.get_messages(&session.session_id).await?;
        
        // 返回最后 limit 条
        let start = messages.len().saturating_sub(limit);
        Ok(messages[start..].to_vec())
    }
    
    /// 列出 Chat Room 的所有 Sessions
    pub async fn list_sessions(
        &self,
        chat_room_id: &str,
        include_archived: bool,
    ) -> Result<Vec<Session>> {
        let index = self.load_index().await?;
        
        let sessions = index
            .chat_rooms
            .get(chat_room_id)
            .map(|room| {
                room.sessions
                    .iter()
                    .filter(|s| include_archived || s.status == SessionStatus::Active)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        
        Ok(sessions)
    }
    
    /// 归档 Session
    async fn archive_session(&self, session: &Session) -> Result<()> {
        let now = Utc::now();
        
        // 1. 更新 session 状态
        let room_dir = self.workspace_path.join(&session.chat_room_id);
        let session_path = room_dir.join(format!("{}.json", session.session_id.to_string()));
        
        let content = fs::read_to_string(&session_path).await?;
        let mut session_data: serde_json::Value = serde_json::from_str(&content)?;
        
        session_data["status"] = json!("archived");
        session_data["archived_at"] = json!(now);
        
        // 生成摘要（可选：调用 LLM 生成）
        // let summary = self.generate_summary(&session).await?;
        // session_data["metadata"]["summary"] = json!(summary);
        
        fs::write(&session_path, serde_json::to_string_pretty(&session_data)?).await?;
        
        // 2. 更新索引
        self.update_session_status(&session.chat_room_id, &session.session_id, SessionStatus::Archived).await?;
        
        // 3. 清除缓存
        self.cache.write().active_sessions.remove(&session.chat_room_id);
        self.cache.write().recent_messages.remove(&session.session_id);
        
        Ok(())
    }
}
```

## 5. 工作流程

### 5.1 正常对话流程

```
用户: "帮我分析代码库"

Chat Room:
  1. 调用 session_manager.get_or_create_active("discord-main", "user#1234")
     → 返回当前活跃 Session
  
  2. 调用 session_manager.add_message(session_id, user_message)
     → 保存用户消息
  
  3. 调用 session_manager.get_recent_messages("discord-main", 20)
     → 获取最近 20 条消息作为上下文
  
  4. Agent 处理（带上上下文）
  
  5. 调用 session_manager.add_message(session_id, agent_message)
     → 保存 Agent 回复
```

### 5.2 Session 重置流程

```
用户: "/reset"

Chat Room:
  1. 检测命令
  
  2. 调用 session_manager.reset_session("discord-main", "user#1234")
     
     内部处理:
     a. 获取当前活跃 Session (Session A)
     b. 归档 Session A (状态改为 archived)
     c. 创建新的 Session B
     d. 更新 active.json 指向 Session B
  
  3. 回复用户: "✅ Session 已重置，开始新的对话"
  
  4. 后续消息都进入 Session B，Session A 保留为历史
```

### 5.3 历史 Session 查询

```
用户: "/history" 或 "/sessions"

Chat Room:
  1. 调用 session_manager.list_sessions("discord-main", true)
     → 返回所有 Sessions（包括已归档）
  
  2. Agent 格式化输出:
     "当前 Chat Room 的历史 Session:
      
      [活跃] Session #abc123 (2024-01-15 14:00)
      - 15 条消息
      - 主题: 代码库分析
      
      [已归档] Session #def456 (2024-01-15 10:30)
      - 42 条消息
      - 主题: 项目初始化
      
      [已归档] Session #ghi789 (2024-01-15 09:00)
      - 8 条消息
      - 主题: 系统介绍"
```

## 6. 与 Chat Room 集成

### 6.1 Chat Room 更新

```rust
pub struct ChatRoom {
    room_id: String,
    room_type: RoomType,
    session_manager: Arc<SessionManager>,
    agents: Vec<Arc<Agent>>,
}

#[async_trait]
impl ChatRoom {
    async fn handle_message(&self, msg: RoomMessage) {
        // 1. 检查是否为 /reset 命令
        if msg.text.trim() == "/reset" {
            let new_session = self.session_manager
                .reset_session(&self.room_id, &msg.author_id)
                .await?;
            
            self.send_message(&format!(
                "✅ Session 已重置\n新的 Session ID: #{}\n开始新的对话吧！",
                new_session.session_id.short()
            )).await?;
            return;
        }
        
        // 2. 检查其他命令
        if msg.text.trim() == "/history" {
            let sessions = self.session_manager
                .list_sessions(&self.room_id, true)
                .await?;
            
            let response = self.format_sessions(&sessions);
            self.send_message(&response).await?;
            return;
        }
        
        // 3. 正常消息处理
        let session = self.session_manager
            .get_or_create_active(&self.room_id, &msg.author_id)
            .await?;
        
        // 保存用户消息
        let user_message = Message::new_user(&msg.author_id, &msg.author_name, &msg.text);
        self.session_manager
            .add_message(&self.room_id, user_message)
            .await?;
        
        // 获取上下文
        let context = self.session_manager
            .get_recent_messages(&self.room_id, 20)
            .await?;
        
        // 分发给 Agent 处理（带上上下文）
        for agent in &self.agents {
            if agent.is_mentioned(&msg) {
                let response = agent.process_with_context(&context, &msg.text).await?;
                
                // 保存 Agent 回复
                let agent_message = Message::new_agent(&agent.id, &response);
                self.session_manager
                    .add_message(&self.room_id, agent_message)
                    .await?;
                
                self.send_message(&response).await?;
            }
        }
    }
}
```

## 7. 与 Agent 集成

### 7.1 Agent 使用 Session 上下文

```rust
impl Agent {
    /// 使用 Session 上下文处理消息
    async fn process_with_context(
        &self,
        context: &[Message],
        input: &str,
    ) -> Result<String> {
        // 1. 构建包含历史上下文的 Prompt
        let prompt = self.build_prompt_with_history(context, input);
        
        // 2. 调用 LLM
        let response = self.llm.complete(prompt).await?;
        
        Ok(response)
    }
    
    /// 构建 Prompt
    fn build_prompt_with_context(&self, context: &[Message], input: &str) -> String {
        let mut prompt = format!("{}", self.soul_md);
        
        prompt.push_str("\n\n## 对话历史\n");
        for msg in context {
            let sender = match &msg.sender {
                Sender::User { name, .. } => format!("用户({})", name),
                Sender::Agent(agent_id) => format!("Agent({})", agent_id),
                Sender::System => "系统".to_string(),
            };
            prompt.push_str(&format!("{}: {}\n", sender, msg.content));
        }
        
        prompt.push_str(&format!("\n## 当前输入\n用户: {}\n", input));
        prompt.push_str("\n请回复:\n");
        
        prompt
    }
}
```

## 8. 配置选项

```toml
# CONFIG.toml
[session]
# 单个 Session 最大消息数（达到后自动建议 reset）
max_messages_per_session = 1000

# 默认加载到 LLM 上下文的最近消息数
context_window_messages = 20

# 是否自动归档不活跃的 Session（0 表示不自动归档）
auto_archive_after_hours = 0

# 存储格式: json | jsonl
storage_format = "json"

# 是否启用内存缓存
cache_enabled = true

# 缓存最近消息数量
cache_message_limit = 100

# 是否自动为归档 Session 生成摘要（需要 LLM 调用）
auto_generate_summary = false
```

## 9. 命令参考

### 9.1 用户命令

| 命令 | 说明 |
|------|------|
| `/reset` | 重置当前 Session，创建新的 Session，旧的归档 |
| `/history` | 显示当前 Chat Room 的所有 Session 历史 |
| `/session` | 显示当前活跃 Session 信息 |
| `/summary` | 为当前 Session 生成摘要（可选） |

### 9.2 程序接口

```rust
// 主要接口
impl SessionManager {
    // 核心操作
    async fn get_or_create_active(&self, chat_room_id: &str, created_by: &str) -> Result<Session>;
    async fn reset_session(&self, chat_room_id: &str, reset_by: &str) -> Result<Session>;
    async fn add_message(&self, chat_room_id: &str, message: Message) -> Result<()>;
    
    // 查询操作
    async fn get_messages(&self, session_id: &SessionId) -> Result<Vec<Message>>;
    async fn get_recent_messages(&self, chat_room_id: &str, limit: usize) -> Result<Vec<Message>>;
    async fn list_sessions(&self, chat_room_id: &str, include_archived: bool) -> Result<Vec<Session>>;
    
    // 管理操作
    async fn archive_session(&self, session: &Session) -> Result<()>;
    async fn generate_summary(&self, session: &Session) -> Result<String>;
}
```

## 10. 设计要点

1. **历史保留**: `/reset` 不会清空消息，而是创建新的 Session，旧的保留为历史
2. **多 Session**: 每个 Chat Room 可以有无限个历史 Session
3. **上下文控制**: 可以配置加载多少条历史消息到 LLM 上下文
4. **持久化**: 所有 Session 和消息都持久化到文件系统
5. **可检索**: 支持按 Session 查看历史对话
6. **轻量索引**: 使用 index.json 快速查找 Session 元数据
7. **内存缓存**: 可选的内存缓存提升热点数据访问性能
8. **扩展性**: 支持未来添加更多命令和功能（如搜索、导出等）

## 11. 相关文档

- [Chat Room 系统](chat-room.md) - 与 Session 的集成
- [Agent 定义系统](agent-definition.md) - Agent 如何使用 Session
- [任务系统](task-system.md) - 任务与 Session 的关系
