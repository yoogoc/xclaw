# Mention 协议系统

## 1. 概述

Mention 协议系统负责将不同 Channel 的 mention 格式统一转换为标准格式，便于 LLM 理解和处理。

**核心职责**:
- 解析不同 Channel 的 mention 格式
- 将 mention 转换为统一的 LLM 格式（`<<@agent_id>>`）
- 管理 user_id ↔ agent_id 的映射关系

**重要**: LLM 格式 `<<@agent_id>>` **直接发送到 Channel**，无需转换回 Channel 原生格式。

## 2. 协议格式

### 2.1 Channel 原生格式（接收时解析）

| Channel | 格式 | 示例 |
|---------|------|------|
| Discord | `<@user_id>` | `<@123456789>` |
| Discord (Nickname) | `<@!user_id>` | `<@!123456789>` |
| Slack | `<@Uxxxxx>` | `<@U0123456789>` |
| Slack (Group) | `<!subteam^Sxxxxx>` | `<!subteam^S012345>` |
| Matrix | `@user:server` | `@agent-a:matrix.org` |

### 2.2 统一 LLM 格式

```
<<@agent_id>> 消息内容
```

**示例**:
```
<<@agent-b>> 请帮我分析这个代码库
```

**发送规则**: LLM 生成的 `<<@agent_id>>` 格式直接发送到 Channel，不进行任何转换。

### 2.3 完整消息格式（发送给 LLM）

```
<<@agent-a>> 请帮我分析这个代码库
```

## 3. 核心架构

### 3.1 Mention Protocol Trait

```rust
// src/mention/mod.rs

/// Mention 协议处理器
pub trait MentionProtocol: Send + Sync {
    /// Channel 类型
    fn channel_type(&self) -> ChannelType;

    /// 解析消息中的 mentions
    /// 返回: (原始格式, agent_id) 列表
    fn parse_mentions(&self, text: &str) -> Vec<(String, String)>;

    /// 将 Channel 格式转换为标准 LLM 格式
    fn to_llm_format(&self, text: &str) -> String;
}

/// Mention 管理器
pub struct MentionManager {
    protocols: HashMap<ChannelType, Box<dyn MentionProtocol>>,
    // user_id -> agent_id 映射
    bindings: HashMap<(ChannelType, String), String>,
}
```

### 3.2 Discord 协议实现

```rust
pub struct DiscordMentionProtocol;

impl MentionProtocol for DiscordMentionProtocol {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Discord
    }

    fn parse_mentions(&self, text: &str, bindings: &HashMap<String, String>) -> Vec<(String, String)> {
        let mut mentions = vec![];
        let re = Regex::new(r"<@!?(\d+)>").unwrap();

        for cap in re.captures_iter(text) {
            let full_match = cap[0].to_string();
            let user_id = &cap[1];

            if let Some(agent_id) = bindings.get(user_id) {
                mentions.push((full_match, agent_id.clone()));
            }
        }

        mentions
    }

    fn to_llm_format(&self, text: &str, bindings: &HashMap<String, String>) -> String {
        let mut result = text.to_string();
        let mentions = self.parse_mentions(text, bindings);

        for (original, agent_id) in mentions {
            let replacement = format!("<<@{}>>", agent_id);
            result = result.replace(&original, &replacement);
        }

        result
    }
}
```

### 3.3 Slack 协议实现

```rust
pub struct SlackMentionProtocol;

impl MentionProtocol for SlackMentionProtocol {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Slack
    }

    fn parse_mentions(&self, text: &str, bindings: &HashMap<String, String>) -> Vec<(String, String)> {
        let mut mentions = vec![];
        let re = Regex::new(r"<@(U[A-Z0-9]+)>").unwrap();

        for cap in re.captures_iter(text) {
            let full_match = cap[0].to_string();
            let user_id = &cap[1];

            if let Some(agent_id) = bindings.get(user_id) {
                mentions.push((full_match, agent_id.clone()));
            }
        }

        mentions
    }

    fn to_llm_format(&self, text: &str, bindings: &HashMap<String, String>) -> String {
        let mut result = text.to_string();
        let mentions = self.parse_mentions(text, bindings);

        for (original, agent_id) in mentions {
            let replacement = format!("<<@{}>>", agent_id);
            result = result.replace(&original, &replacement);
        }

        result
    }
}
```

### 3.4 Matrix 协议实现

```rust
pub struct MatrixMentionProtocol;

impl MentionProtocol for MatrixMentionProtocol {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Matrix
    }

    fn parse_mentions(&self, text: &str, bindings: &HashMap<String, String>) -> Vec<(String, String)> {
        let mut mentions = vec![];
        let re = Regex::new(r"@([a-zA-Z0-9_-]+:[a-zA-Z0-9.-]+)").unwrap();

        for cap in re.captures_iter(text) {
            let full_match = cap[0].to_string();

            if let Some(agent_id) = bindings.get(&full_match) {
                mentions.push((full_match, agent_id.clone()));
            }
        }

        mentions
    }

    fn to_llm_format(&self, text: &str, bindings: &HashMap<String, String>) -> String {
        let mut result = text.to_string();
        let mentions = self.parse_mentions(text, bindings);

        for (original, agent_id) in mentions {
            let replacement = format!("<<@{}>>", agent_id);
            result = result.replace(&original, &replacement);
        }

        result
    }
}
```

## 4. 消息转换流程

### 4.1 接收消息（Channel → LLM）

```rust
impl MentionManager {
    /// 将 Channel 消息转换为 LLM 格式
    pub fn to_llm_message(
        &self,
        channel_type: ChannelType,
        raw_text: &str,
        author: &str,
    ) -> LLMMessage {
        // 1. 获取协议处理器
        let protocol = self.get_protocol(channel_type);

        // 2. 获取该 channel 的 bindings
        let bindings = self.get_bindings(channel_type);

        // 3. 转换格式
        let llm_content = protocol.to_llm_format(raw_text, &bindings);

        // 4. 解析 mentions
        let mentions = protocol.parse_mentions(raw_text, &bindings);

        LLMMessage {
            content: llm_content,
            mentions: mentions.into_iter().map(|(_, id)| id).collect(),
        }
    }
}
```

**转换示例**:

```
原始 Discord 消息:
  <@123456789> 请帮我分析这个代码库

转换后（发送给 LLM）:
  <<@agent-a>> 请帮我分析这个代码库
```

### 4.2 发送消息（LLM → Channel）

**重要**: 无需转换，直接发送 LLM 格式。

```rust
impl ChatRoom {
    /// 发送消息（直接使用 LLM 格式）
    pub async fn send(&self, message: &str) -> Result<()> {
        // LLM 格式: "<<@agent-b>> 新任务 #a7b3c9d2..."
        // 直接发送，不做任何转换
        self.channel.send(message).await
    }
}
```

**示例**:

```
LLM 生成:
  <<@agent-b>> 新任务 #a7b3c9d2：读取文件结构

发送到 Discord（直接发送）:
  <<@agent-b>> 新任务 #a7b3c9d2：读取文件结构

发送到 Slack（直接发送）:
  <<@agent-b>> 新任务 #a7b3c9d2：读取文件结构
```

## 5. 配置绑定

```toml
# config.toml

[[chat_rooms]]
id = "main-collaboration"
type = "discord"
token = "${DISCORD_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "111111111"
# Discord User ID 用于解析收到的 mention
discord_user_id = "123456789"

[[chat_rooms.bindings]]
agent_id = "agent-b"
channel_id = "222222222"
discord_user_id = "987654321"

---

[[chat_rooms]]
id = "slack-workspace"
type = "slack"
token = "${SLACK_TOKEN}"

[[chat_rooms.bindings]]
agent_id = "agent-a"
channel_id = "C1234567890"
slack_user_id = "U0123456789"

[[chat_rooms.bindings]]
agent_id = "agent-c"
channel_id = "C0987654321"
slack_user_id = "U9876543210"
```

## 6. 完整示例

### 6.1 用户 → Agent A（接收流程）

```
[Discord 用户发送]
  <@123456789> 请帮我分析这个代码库

[Chat Room 接收]
  原始内容: <@123456789> 请帮我分析这个代码库
  Channel: Discord
  ChatRoom: main-collaboration

[Mention Protocol 处理]
  1. 使用 Discord 协议解析
  2. 发现 mention: <@123456789>
  3. 查表: 123456789 -> agent-a
  4. 转换: <@123456789> -> <<@agent-a>>

[发送给 LLM]
  <<@agent-a>> 请帮我分析这个代码库

[Agent A LLM 处理]
  识别到: <<@agent-a>> 是自己
  生成回复: <<@agent-b>> 新任务 #a7b3c9d2...
```

### 6.2 Agent A → Agent B（发送流程）

```
[Agent A LLM 生成回复]
  <<@agent-b>> 新任务 #a7b3c9d2：读取文件结构
  分析代码库的目录组织和文件分布

[Chat Room 直接发送]
  内容: <<@agent-b>> 新任务 #a7b3c9d2：读取文件结构...
  ⚠️ 注意: 直接发送，不转换 <@987654321>

[Discord 显示]
  <<@agent-b>> 新任务 #a7b3c9d2：读取文件结构

[Agent B 接收]
  看到: <<@agent-b>>
  识别到是自己（通过字符串匹配 agent_id）
  处理任务
```

### 6.3 跨平台通信

```
[Agent A 在 Discord 生成消息]
  <<@agent-c>> 任务 #a7b3c9d2 需要你协助

[通过 Slack Chat Room 发送]
  内容: <<@agent-c>> 任务 #a7b3c9d2 需要你协助
  ⚠️ 直接发送，不转换格式

[Slack 显示]
  <<@agent-c>> 任务 #a7b3c9d2 需要你协助

[Agent C 在 Slack 接收]
  识别到 <<@agent-c>> 是自己
  回复: <<@agent-a>> 收到，开始处理

[通过 Discord Chat Room 回复]
  内容: <<@agent-a>> 收到，开始处理
```

## 7. 设计要点

### 7.1 单向转换

- **接收**: Channel 格式 → LLM 格式（必须转换）
- **发送**: LLM 格式 → 直接发送（无需转换）

### 7.2 Agent 识别方式

Agent 通过以下方式识别消息是否提到自己：

1. **LLM 识别**: 从 `<<@agent_id>>` 中提取 agent_id
2. **字符串匹配**: 在 SOUL.md 中指导 LLM 识别 `<<@xxx>>` 格式
3. **无需 Channel 原生 mention**: 不依赖 `<@user_id>` 格式

### 7.3 优势

- ✅ **简单**: 无需双向转换逻辑
- ✅ **一致**: 所有 Channel 使用相同的显示格式
- ✅ **可扩展**: 新增 Channel 只需实现解析逻辑
- ✅ **透明**: 人类和 LLM 都使用相同格式

### 7.4 限制

- ⚠️ 不会触发 Channel 的原生 mention 通知（如 Discord 的高亮）
- ⚠️ 需要 Agent 通过字符串匹配识别自己（而非原生 mention 机制）

## 8. 错误处理

```rust
pub enum MentionError {
    ProtocolNotFound(ChannelType),
    UnknownMention { user_id: String, channel: ChannelType },
    InvalidFormat(String),
}
```

## 9. 相关文档

- [Chat Room 系统](chat-room.md) - Chat Room 抽象层
- [通信协议](communication.md) - 消息格式规范
- [Agent 定义系统](agent-definition.md) - SOUL.md 配置
