# Chat Room 系统

## 1. 概述

Chat Room 是一个抽象概念，代表一个特定的多 Agent 协作环境。它不依赖于具体的通信协议，可以是：

- **Discord**: 一个 Server 或特定的 Channel
- **Slack**: 一个 Workspace 或 Channel
- **Matrix**: 一个 Room
- **未来扩展**: 任何支持多用户聊天的平台

## 2. 核心概念

### 2.1 Chat Room 抽象

```rust
/// Chat Room 统一接口
#[async_trait]
pub trait ChatRoom: Send + Sync {
    /// Chat Room 类型
    fn room_type(&self) -> RoomType;
    
    /// Chat Room ID
    fn room_id(&self) -> &str;
    
    /// 启动 Chat Room（建立连接）
    async fn start(&self) -> Result<()>;
    
    /// 向指定 Agent 发送消息
    async fn send_to(&self, agent_id: &str, message: RoomMessage) -> Result<()>;
    
    /// 广播消息给所有在线 Agent
    async fn broadcast(&self, message: RoomMessage) -> Result<()>;
    
    /// 接收消息流
    fn receive(&self) -> mpsc::Receiver<RoomEvent>;
    
    /// 获取绑定到此 Chat Room 的所有 Agent
    fn bound_agents(&self) -> Vec<String>;
}

pub enum RoomType {
    Discord,
    Slack,
    Matrix,
    // 可扩展: Telegram, WhatsApp, etc.
}

/// 通用消息格式（与具体协议无关）
pub struct RoomMessage {
    pub text: String,              // 消息文本（包含 @mentions）
    pub reply_to: Option<String>,  // 回复的消息ID
    pub metadata: MessageMetadata,
}

pub struct MessageMetadata {
    pub sender: String,            // 发送者 Agent ID
    pub timestamp: DateTime<Utc>,
}

/// Chat Room 事件
pub enum RoomEvent {
    MessageReceived {
        id: String,                // 消息ID（协议相关）
        text: String,
        author: String,            // 发送者（Agent ID 或用户ID）
        timestamp: DateTime<Utc>,
    },
    AgentJoined {
        agent_id: String,
    },
    AgentLeft {
        agent_id: String,
    },
}
```

### 2.2 与 Channel 的关系

```
Chat Room（抽象层）
    │
    ├── Discord Chat Room
    │       └── 管理多个 Discord Channel bindings
    │
    ├── Slack Chat Room
    │       └── 管理多个 Slack Channel bindings
    │
    └── Matrix Chat Room
            └── 管理多个 Matrix Room bindings
```

**关键区别**:
- **Chat Room**: 逻辑概念，代表一个协作空间
- **Binding**: 物理连接，指定 Agent 在 Chat Room 中的具体位置

## 3. 配置系统

### 3.1 Chat Room 配置

```toml
# config.toml

[[chat_rooms]]
id = "main-collaboration"      # Chat Room 唯一标识
type = "discord"               # 类型: discord | slack | matrix

# Discord 特有配置
token = "${DISCORD_BOT_TOKEN}"
guild_id = "123456789"         # Server ID（可选，如不指定则通过 bindings 推断）

# 绑定配置：哪些 Agent 在这个 Chat Room 的哪个 Channel
[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "111111111"       # Discord Channel ID

[[chat_rooms.bindings]]
agent_id = "agent-b"
channel_id = "222222222"

[[chat_rooms.bindings]]
agent_id = "agent-c"
channel_id = "333333333"

---

[[chat_rooms]]
id = "slack-workspace"
type = "slack"

# Slack 特有配置
token = "${SLACK_BOT_TOKEN}"
workspace_name = "my-team"

[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "C1234567890"     # Slack Channel ID

[[chat_rooms.bindings]]
agent_id = "agent-c"
channel_id = "C0987654321"

---

[[chat_rooms]]
id = "matrix-dev-room"
type = "matrix"

# Matrix 特有配置
homeserver = "https://matrix.org"
user_id = "@agent-bot:matrix.org"
access_token = "${MATRIX_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-b"
room_id = "!dev-room:matrix.org"

[[chat_rooms.bindings]]
agent_id = "agent-c"
room_id = "!dev-room:matrix.org"
```

### 3.2 Agent 配置引用 Chat Room

```toml
# Agent 配置（CONFIG.toml）

[agent]
id = "agent-a"
name = "Agent A"
display_name = "A"

[channels]
# Agent 可以存在于多个 Chat Room
chat_rooms = ["main-collaboration", "slack-workspace"]
# 或指定默认主 Chat Room
primary_room = "main-collaboration"
```

## 4. 具体实现

### 4.1 Discord Chat Room

```rust
pub struct DiscordChatRoom {
    room_id: String,
    token: String,
    guild_id: Option<String>,
    bindings: HashMap<String, ChannelId>,  // agent_id -> channel_id
    http: Option<Arc<Http>>,
    event_sender: mpsc::Sender<RoomEvent>,
}

#[async_trait]
impl ChatRoom for DiscordChatRoom {
    fn room_type(&self) -> RoomType {
        RoomType::Discord
    }
    
    fn room_id(&self) -> &str {
        &self.room_id
    }
    
    async fn start(&self) -> Result<()> {
        // 启动 Discord bot
        let intents = GatewayIntents::GUILD_MESSAGES 
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;
        
        let mut client = Client::builder(&self.token, intents)
            .event_handler(DiscordHandler {
                bindings: self.bindings.clone(),
                event_sender: self.event_sender.clone(),
            })
            .await?;
        
        client.start().await?;
        Ok(())
    }
    
    async fn send_to(&self, agent_id: &str, message: RoomMessage) -> Result<()> {
        let channel_id = self.bindings
            .get(agent_id)
            .ok_or_else(|| Error::AgentNotBound(agent_id.to_string()))?;
        
        let http = self.http.as_ref().ok_or(Error::NotStarted)?;
        
        channel_id.send_message(&http, |m| {
            m.content(&message.text)
        }).await?;
        
        Ok(())
    }
    
    async fn broadcast(&self, message: RoomMessage) -> Result<()> {
        // 向所有绑定的 Agent 发送
        for (agent_id, _) in &self.bindings {
            self.send_to(agent_id, message.clone()).await?;
        }
        Ok(())
    }
    
    fn receive(&self) -> mpsc::Receiver<RoomEvent> {
        // 返回事件接收端
        let (tx, rx) = mpsc::channel(100);
        rx
    }
    
    fn bound_agents(&self) -> Vec<String> {
        self.bindings.keys().cloned().collect()
    }
}

/// Discord 事件处理器
struct DiscordHandler {
    bindings: HashMap<String, ChannelId>,
    event_sender: mpsc::Sender<RoomEvent>,
}

#[async_trait]
impl EventHandler for DiscordHandler {
    async fn message(&self, ctx: Context, msg: Message) {
        // 找到发送者对应的 Agent ID
        let author_agent = self.find_agent_by_author(&msg.author.id);
        
        let event = RoomEvent::MessageReceived {
            id: msg.id.0.to_string(),
            text: msg.content,
            author: author_agent.unwrap_or_else(|| msg.author.name),
            timestamp: Utc::now(),
        };
        
        let _ = self.event_sender.send(event).await;
    }
}
```

### 4.2 Slack Chat Room

```rust
pub struct SlackChatRoom {
    room_id: String,
    token: String,
    workspace: String,
    bindings: HashMap<String, String>,  // agent_id -> channel_id
    client: Option<SlackClient>,
    event_sender: mpsc::Sender<RoomEvent>,
}

#[async_trait]
impl ChatRoom for SlackChatRoom {
    fn room_type(&self) -> RoomType {
        RoomType::Slack
    }
    
    async fn start(&self) -> Result<()> {
        // 启动 Slack Socket Mode 或 Events API
        let client = SlackClient::new(&self.token);
        
        // 启动消息监听
        tokio::spawn(async move {
            // Socket Mode 连接
        });
        
        Ok(())
    }
    
    async fn send_to(&self, agent_id: &str, message: RoomMessage) -> Result<()> {
        let channel_id = self.bindings
            .get(agent_id)
            .ok_or_else(|| Error::AgentNotBound(agent_id.to_string()))?;
        
        let client = self.client.as_ref().ok_or(Error::NotStarted)?;
        
        let req = ChatPostMessageRequest {
            channel: channel_id.clone(),
            text: message.text,
            thread_ts: message.reply_to,
            ..Default::default()
        };
        
        client.chat_post_message(&req).await?;
        Ok(())
    }
    
    // ... 其他方法实现
}
```

### 4.3 Matrix Chat Room

```rust
pub struct MatrixChatRoom {
    room_id: String,
    homeserver: String,
    user_id: String,
    access_token: String,
    bindings: HashMap<String, OwnedRoomId>,  // agent_id -> room_id
    client: Option<MatrixClient>,
    event_sender: mpsc::Sender<RoomEvent>,
}

#[async_trait]
impl ChatRoom for MatrixChatRoom {
    fn room_type(&self) -> RoomType {
        RoomType::Matrix
    }
    
    async fn start(&self) -> Result<()> {
        // 启动 Matrix 客户端
        let client = MatrixClient::new(&self.homeserver).await?;
        client.login(&self.user_id, &self.access_token).await?;
        
        // 开始同步
        client.sync().await?;
        
        Ok(())
    }
    
    async fn send_to(&self, agent_id: &str, message: RoomMessage) -> Result<()> {
        let room_id = self.bindings
            .get(agent_id)
            .ok_or_else(|| Error::AgentNotBound(agent_id.to_string()))?;
        
        let client = self.client.as_ref().ok_or(Error::NotStarted)?;
        
        let content = RoomMessageEventContent::text_plain(message.text);
        client.send_room_message(room_id, content).await?;
        
        Ok(())
    }
    
    // ... 其他方法实现
}
```

## 5. Chat Room 管理器

```rust
pub struct ChatRoomManager {
    rooms: HashMap<String, Box<dyn ChatRoom>>,
    agent_rooms: HashMap<String, Vec<String>>,  // agent_id -> [room_id]
}

impl ChatRoomManager {
    /// 注册 Chat Room
    pub fn register(&mut self, room: Box<dyn ChatRoom>) {
        let room_id = room.room_id().to_string();
        
        // 记录 Agent 绑定关系
        for agent_id in room.bound_agents() {
            self.agent_rooms
                .entry(agent_id)
                .or_default()
                .push(room_id.clone());
        }
        
        self.rooms.insert(room_id, room);
    }
    
    /// 获取 Agent 所在的 Chat Rooms
    pub fn get_agent_rooms(&self, agent_id: &str) -> Vec<&dyn ChatRoom> {
        self.agent_rooms
            .get(agent_id)
            .map(|rooms| {
                rooms.iter()
                    .filter_map(|id| self.rooms.get(id).map(|r| r.as_ref()))
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// 向特定 Agent 发送消息（自动路由到正确的 Chat Room）
    pub async fn send_to_agent(
        &self,
        agent_id: &str,
        message: RoomMessage,
    ) -> Result<()> {
        let rooms = self.get_agent_rooms(agent_id);
        
        if rooms.is_empty() {
            return Err(Error::AgentNotInAnyRoom(agent_id.to_string()));
        }
        
        // 使用第一个可用的 Chat Room
        rooms[0].send_to(agent_id, message).await
    }
    
    /// 广播到所有 Chat Room
    pub async fn broadcast_all(&self, message: RoomMessage) -> Result<()> {
        for room in self.rooms.values() {
            room.broadcast(message.clone()).await?;
        }
        Ok(())
    }
    
    /// 启动所有 Chat Rooms
    pub async fn start_all(&self) -> Result<()> {
        for room in self.rooms.values() {
            room.start().await?;
        }
        Ok(())
    }
}
```

## 6. 使用示例

### 6.1 启动系统

```rust
async fn main() -> Result<()> {
    // 1. 加载配置
    let config = load_config("config.toml").await?;
    
    // 2. 创建 Chat Room Manager
    let mut manager = ChatRoomManager::new();
    
    // 3. 根据配置创建 Chat Rooms
    for room_config in config.chat_rooms {
        let room: Box<dyn ChatRoom> = match room_config.room_type {
            RoomType::Discord => {
                Box::new(DiscordChatRoom::from_config(room_config))
            },
            RoomType::Slack => {
                Box::new(SlackChatRoom::from_config(room_config))
            },
            RoomType::Matrix => {
                Box::new(MatrixChatRoom::from_config(room_config))
            },
        };
        
        manager.register(room);
    }
    
    // 4. 启动所有 Chat Rooms
    manager.start_all().await?;
    
    // 5. 创建 Agents
    let agents = create_agents(&config, &manager).await?;
    
    // 6. 运行
    run_agents(agents).await?;
    
    Ok(())
}
```

### 6.2 Agent 发送通知

```rust
impl Agent {
    pub async fn notify(&self, to: &str, message: &str, task_id: Option<TaskId>) {
        // 构建包含 task_id 的消息
        let text = if let Some(id) = task_id {
            format!("@{} 任务 #{}：{}", to, id.short(), message)
        } else {
            format!("@{} {}", to, message)
        };
        
        let room_message = RoomMessage {
            text,
            reply_to: None,
            metadata: MessageMetadata {
                sender: self.id.clone(),
                timestamp: Utc::now(),
            },
        };
        
        // 通过 Chat Room Manager 发送
        self.chat_room_manager
            .send_to_agent(to, room_message)
            .await;
    }
}
```

### 6.3 多 Chat Room 场景

```
Chat Room: main-collaboration (Discord)
├── Agent A (channel: general)
├── Agent B (channel: coding)
└── Agent C (channel: coding)

Chat Room: slack-workspace (Slack)
├── Agent A (channel: dev-general)
└── Agent C (channel: dev-backend)

Chat Room: matrix-dev-room (Matrix)
├── Agent B (room: dev-room)
└── Agent C (room: dev-room)

Agent A 分配任务给 Agent B:
→ 通过 main-collaboration (Discord) 发送

Agent B 完成任务通知 Agent A:
→ 通过 main-collaboration (Discord) 回复

Agent A 同时通知 Agent C (在 Slack):
→ 通过 slack-workspace (Slack) 发送
```

## 7. 配置示例详解

### 7.1 单平台多 Agent

```toml
# 所有 Agent 在同一个 Discord Server 的不同 Channel
[[chat_rooms]]
id = "discord-team"
type = "discord"
token = "${DISCORD_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "111111111"  # #general

[[chat_rooms.bindings]]
agent_id = "agent-b"  
channel_id = "222222222"  # #coding

[[chat_rooms.bindings]]
agent_id = "agent-c"
channel_id = "222222222"  # #coding (和 B 同频道)
```

### 7.2 跨平台混合

```toml
# Discord 作为主协作平台
[[chat_rooms]]
id = "primary-discord"
type = "discord"
token = "${DISCORD_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "111111111"

[[chat_rooms.bindings]]
agent_id = "agent-b"
channel_id = "222222222"

# Slack 作为补充
[[chat_rooms]]
id = "backup-slack"
type = "slack"
token = "${SLACK_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "C1234567890"

[[chat_rooms.bindings]]
agent_id = "agent-c"
channel_id = "C1234567890"
```

### 7.3 复杂多平台场景

```toml
# 开发团队 Discord
[[chat_rooms]]
id = "dev-discord"
type = "discord"
token = "${DEV_DISCORD_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "dev-general"

[[chat_rooms.bindings]]
agent_id = "agent-b"
channel_id = "dev-coding"

# 运维团队 Slack
[[chat_rooms]]
id = "ops-slack"
type = "slack"
token = "${OPS_SLACK_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "ops-alerts"

[[chat_rooms.bindings]]
agent_id = "agent-c"
channel_id = "ops-general"

# 开源社区 Matrix
[[chat_rooms]]
id = "community-matrix"
type = "matrix"
homeserver = "https://matrix.org"
access_token = "${MATRIX_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-b"
room_id = "!community:matrix.org"
```

## 8. 扩展新平台

添加新的 Chat Room 类型只需：

1. **实现 ChatRoom trait**
2. **添加配置解析**
3. **注册到 RoomType enum**

示例（添加 Telegram）：

```rust
// 1. 实现 trait
pub struct TelegramChatRoom { ... }

#[async_trait]
impl ChatRoom for TelegramChatRoom {
    // ... 实现接口
}

// 2. 添加到配置
#[derive(Deserialize)]
pub enum RoomType {
    Discord,
    Slack,
    Matrix,
    Telegram,  // 新增
}

// 3. 创建实例
match config.room_type {
    RoomType::Telegram => {
        Box::new(TelegramChatRoom::from_config(config))
    },
    // ...
}
```

## 9. 相关文档

- [概述](README.md) - 系统架构总览
- [Agent 定义系统](agent-definition.md) - SOUL.md 和 TOOL.md
- [任务系统](task-system.md) - Task ID 和任务生命周期
- [心跳机制](heartbeat.md) - 停滞检测
- [工作流程](workflows.md) - 完整交互流程
