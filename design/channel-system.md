# Channel 系统设计

## 1. 概述

Channel 是外部通信平台的抽象层，负责：
- 接收来自外部平台的消息
- 发送响应到外部平台
- 处理平台特定的消息格式转换
- 支持流式输出（draft updates）

## 2. 核心架构

```
┌─────────────────────────────────────────────────────────┐
│                    Channel Trait                        │
│  - receive() -> MessageStream                           │
│  - send(message) -> Result<()>                          │
│  - supports_draft_updates() -> bool                     │
│  - send_draft(message) -> Result<()>                    │
└─────────────────────────────────────────────────────────┘
                          ▲
                          │
          ┌───────────────┴───────────────┐
          │                               │
┌─────────────────────┐       ┌─────────────────────┐
│  WebSocketChannel   │       │   DiscordChannel    │
│  - connections      │       │   - client          │
│  - supports draft   │       │   - no draft        │
└─────────────────────┘       └─────────────────────┘
```

## 3. Channel Trait 设计

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    /// 接收消息流
    async fn receive(&self) -> Result<MessageStream>;

    /// 发送完整消息
    async fn send(&self, message: OutgoingMessage) -> Result<()>;

    /// 是否支持 draft updates（流式输出）
    fn supports_draft_updates(&self) -> bool;

    /// 发送 draft 更新（仅支持的平台）
    async fn send_draft(&self, message: OutgoingMessage) -> Result<()> {
        if !self.supports_draft_updates() {
            return Err(anyhow!("Draft updates not supported"));
        }
        Ok(())
    }

    /// 平台名称
    fn platform(&self) -> &str;
}
```

## 4. WebSocket Channel 设计

### 4.1 特性
- 支持 draft updates（流式输出）
- 双向实时通信
- 连接管理（多客户端）

### 4.2 消息协议

**客户端 → 服务器：**
```json
{
  "type": "message",
  "content": "用户消息内容",
  "user_id": "user123",
  "thread_id": "optional_thread_id",
  "attachments": []
}
```

**服务器 → 客户端：**
```json
{
  "type": "draft",  // or "final"
  "content": "AI 响应内容",
  "thread_id": "thread_id",
  "is_complete": false
}
```

### 4.3 实现要点

```rust
pub struct WebSocketChannel {
    /// 活跃连接
    connections: Arc<RwLock<HashMap<String, WebSocketSender>>>,
    /// 消息接收队列
    message_rx: Arc<Mutex<mpsc::UnboundedReceiver<IncomingMessage>>>,
    /// 消息发送队列
    message_tx: mpsc::UnboundedSender<IncomingMessage>,
}

impl WebSocketChannel {
    /// 添加新连接
    pub async fn add_connection(&self, user_id: String, ws: WebSocket);

    /// 移除连接
    pub async fn remove_connection(&self, user_id: &str);

    /// 广播消息到特定用户
    async fn send_to_user(&self, user_id: &str, msg: OutgoingMessage);
}
```

## 5. Discord Channel 设计

### 5.1 特性
- 不支持 draft updates
- 基于 serenity 库
- 支持 @mention 检测

### 5.2 实现要点

```rust
pub struct DiscordChannel {
    /// Discord client
    client: Arc<serenity::Client>,
    /// 消息接收队列
    message_rx: Arc<Mutex<mpsc::UnboundedReceiver<IncomingMessage>>>,
    /// 消息发送队列
    message_tx: mpsc::UnboundedSender<IncomingMessage>,
    /// 配置
    config: DiscordConfig,
}

pub struct DiscordConfig {
    /// Bot token
    pub token: String,
    /// 监听的 channel IDs
    pub channel_ids: Vec<String>,
    /// 是否需要 @mention
    pub require_mention: bool,
}
```

## 6. ChannelManager 设计

```rust
pub struct ChannelManager<C: Channel> {
    channel: Arc<C>,
    /// Draft 消息缓冲区（按 thread_id）
    draft_buffer: Arc<RwLock<HashMap<String, String>>>,
}

impl<C: Channel> ChannelManager<C> {
    /// 接收消息
    pub async fn receive(&self) -> Result<MessageStream>;

    /// 发送 chunk（流式）
    pub async fn send_chunk(&self, thread_id: &str, chunk: &str) -> Result<()> {
        if self.channel.supports_draft_updates() {
            // 追加到 buffer
            let mut buffer = self.draft_buffer.write().await;
            let content = buffer.entry(thread_id.to_string())
                .or_insert_with(String::new);
            content.push_str(chunk);

            // 发送 draft
            self.channel.send_draft(OutgoingMessage {
                content: content.clone(),
                thread_id: Some(thread_id.to_string()),
                is_draft: true,
                ..Default::default()
            }).await?;
        }
        // 不支持 draft 的平台：缓存到内存
        Ok(())
    }

    /// 发送最终消息
    pub async fn send_final(&self, thread_id: &str) -> Result<()> {
        let content = {
            let mut buffer = self.draft_buffer.write().await;
            buffer.remove(thread_id).unwrap_or_default()
        };

        self.channel.send(OutgoingMessage {
            content,
            thread_id: Some(thread_id.to_string()),
            is_draft: false,
            ..Default::default()
        }).await
    }
}
```

## 7. 消息类型定义

```rust
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub id: Uuid,
    pub channel: String,        // "websocket" or "discord"
    pub user_id: String,
    pub user_name: Option<String>,
    pub content: String,
    pub thread_id: Option<String>,
    pub received_at: DateTime<Utc>,
    pub attachments: Vec<Attachment>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub content: String,
    pub thread_id: Option<String>,
    pub is_draft: bool,         // draft vs final
    pub attachments: Vec<String>,
    pub metadata: serde_json::Value,
}
```

## 8. 关键设计决策

### 8.1 Draft Updates 处理
- **支持平台（WebSocket）**：实时发送每个 chunk
- **不支持平台（Discord）**：缓存所有 chunks，最后一次性发送

### 8.2 连接管理
- WebSocket：维护活跃连接池，支持断线重连
- Discord：使用 serenity 的内置连接管理

### 8.3 消息路由
- 通过 `channel` 字段区分来源平台
- 通过 `thread_id` 支持多线程对话

### 8.4 错误处理
- 连接断开：自动清理资源
- 发送失败：记录日志，不阻塞主流程
- 格式错误：返回错误消息给用户

## 9. 实现优先级

**Phase 1（最小可用）：**
1. Channel trait 定义
2. WebSocketChannel 基础实现
3. ChannelManager 基础功能

**Phase 2（Discord 支持）：**
1. DiscordChannel 实现
2. @mention 检测
3. Discord 特定格式处理

**Phase 3（增强功能）：**
1. 重连机制
2. 消息队列持久化
3. 速率限制
