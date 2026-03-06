# 通信协议

## 1. 概述

系统通过 Discord 进行 Agent 间通信。所有消息必须显式包含 task_id，代码层面**不做提取**，完全依赖 LLM 的理解能力。

## 2. Discord 消息格式

### 2.1 任务分配消息

**格式模板**:
```
@[target-agent] 新任务 #[short-id]：
**[任务标题]**
[任务描述]

任务ID: [完整-task-id]
优先级: [优先级]
[截止时间信息]

👉 使用 task_read task_id: "[short-id]" 查看详情
```

**示例**:
```
@agent-b 新任务 #a7b3c9d2：
**读取项目文件结构**
分析代码库的目录组织和文件分布，识别主要模块和入口文件

任务ID: a7b3c9d2-e4f5-6789-abcd-ef0123456789
优先级: High
截止时间: 2024-01-15 10:00 UTC

👉 使用 task_read task_id: "a7b3c9d2" 查看详情
```

### 2.2 任务认领确认

**格式模板**:
```
@[dispatcher] 已认领任务 #[short-id]
[可选的补充说明]

预计完成时间: [时间]
```

**示例**:
```
@agent-a 已认领任务 #a7b3c9d2
开始分析代码库结构

预计完成时间: 10 分钟内
```

### 2.3 进度更新消息

**格式模板**:
```
@[dispatcher] 任务 #[short-id] 进度更新
[进度条] [百分比]%
当前步骤: [步骤描述]

[可选的详细信息]
```

**示例**:
```
@agent-a 任务 #a7b3c9d2 进度更新
[██████░░░░] 60%
当前步骤: 分析模块依赖

已扫描 15 个目录，发现 8 个主要模块
```

### 2.4 心跳询问消息（停滞检测）

**格式模板**:
```
@[target-agent] 任务 #[short-id] 已超过 [X] 分钟无进度更新，请确认：
1. 是否仍在执行？
2. 是否遇到阻塞需要协助？
3. 预计何时完成？

上次更新: [时间戳]
```

**示例**:
```
@agent-c 任务 #f8e2d1a3 已超过 15 分钟无进度更新，请确认：
1. 是否仍在执行？
2. 是否遇到阻塞需要协助？
3. 预计何时完成？

上次更新: 2024-01-15 09:05:00 UTC (进度 10%)
```

### 2.5 停滞询问回复（正常执行）

**格式模板**:
```
@[dispatcher] 任务 #[short-id] 仍在执行
[状态说明]

当前实际进度: [百分比]%
预计完成时间: [时间]
```

**示例**:
```
@agent-a 任务 #f8e2d1a3 仍在执行
正在处理复杂的循环依赖关系，比预期复杂

当前实际进度: 40%
预计完成时间: 还需 10 分钟
```

### 2.6 停滞询问回复（需要帮助）

**格式模板**:
```
@[dispatcher] 任务 #[short-id] 遇到阻塞
**问题**: [问题描述]

**已尝试**: [尝试过的解决方案]
**需要帮助**: [具体需要什么帮助]
```

**示例**:
```
@agent-a 任务 #f8e2d1a3 遇到阻塞
**问题**: 无法访问私有仓库 npm.example.com，返回 401 错误

**已尝试**: 
- 重试 3 次
- 检查网络连接正常

**需要帮助**: 需要配置访问 token 或调整任务范围
```

### 2.7 任务完成消息

**格式模板**:
```
@[dispatcher] 任务 #[short-id] 已完成 ✅
**结果摘要**: [一句话总结]

**产出**:
- [产出项 1]
- [产出项 2]

[可选的后续建议]
```

**示例**:
```
@agent-a 任务 #a7b3c9d2 已完成 ✅
**结果摘要**: 成功分析代码库结构，识别 15 个模块

**产出**:
- 文件列表: 42 个源文件
- 模块依赖图已生成
- 发现 3 个循环依赖需要优化

**建议**: 可以开始下一步的详细代码审查
```

### 2.8 任务失败消息

**格式模板**:
```
@[dispatcher] 任务 #[short-id] 执行失败 ❌
**错误**: [错误描述]

**是否可以重试**: [是/否]
[重试建议或替代方案]
```

**示例**:
```
@agent-a 任务 #a7b3c9d2 执行失败 ❌
**错误**: 无法解析项目配置文件，格式不兼容

**是否可以重试**: 否
**建议**: 需要先升级项目配置格式，或调整任务范围跳过此步骤
```

### 2.9 任务重新分配消息

**格式模板**:
```
@[new-agent] 请接手任务 #[short-id]
**原执行者**: [old-agent]
**转手原因**: [原因]

**当前进度**: [百分比]%
**已完成**: [已完成的工作]
**待完成**: [剩余工作]

👉 使用 task_read task_id: "[short-id]" 查看完整详情
```

**示例**:
```
@agent-b 请接手任务 #f8e2d1a3
**原执行者**: agent-c
**转手原因**: 遇到权限问题无法继续

**当前进度**: 40%
**已完成**: 分析了主要模块结构
**待完成**: 需要访问私有仓库获取完整依赖关系

👉 使用 task_read task_id: "f8e2d1a3" 查看完整详情
```

## 3. 消息处理流程

### 3.1 收到消息时的处理

```rust
impl DiscordChannel {
    async fn handle_message(&self, msg: Message) {
        // 1. 检查是否 @ 了本 Agent
        if !self.is_mentioned(&msg) {
            return;
        }
        
        // 2. 提取纯文本（去掉 @mention）
        let content = self.extract_content(&msg);
        
        // 3. 直接交给 Agent 处理
        // LLM 会从内容中识别 task_id 并决定工具调用
        let response = self.agent.process(&AgentInput {
            message: content,
            channel: "discord",
            sender: msg.author.name.clone(),
        }).await;
        
        // 4. 发送回复
        if let Some(reply) = response.reply {
            self.send_message(&reply).await;
        }
    }
    
    /// 仅提取 @mention，保留其他所有内容
    fn extract_content(&self, msg: &Message) -> String {
        // 去掉 "@agent-name" 部分，保留 task_id 和其他内容
        let mut content = msg.content.clone();
        for mention in &msg.mentions {
            if mention.id == self.bot_id {
                content = content.replace(&format!("<@{}>", mention.id), "");
            }
        }
        content.trim().to_string()
    }
}
```

### 3.2 不提取 task_id 的原因

1. **信任 LLM**: LLM 能从文本中识别 #task_id 格式
2. **配置驱动**: 提取逻辑应在 SOUL.md 中定义，而非代码硬编码
3. **灵活性**: 支持多种 task_id 提及方式（#id, task_id: id 等）
4. **简单性**: 代码逻辑更简洁，减少维护负担

## 4. Discord 通道实现

### 4.1 核心结构

```rust
pub struct DiscordChannel {
    agent_config: AgentConfig,  // 包含 SOUL.md 和 TOOL.md
    http: Arc<Http>,
    channel_id: ChannelId,
    workspace: Arc<Workspace>,
}

#[async_trait]
impl EventHandler for DiscordChannel {
    async fn message(&self, ctx: Context, msg: Message) {
        self.handle_message(msg).await;
    }
    
    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }
}
```

### 4.2 发送消息

```rust
impl DiscordChannel {
    pub async fn send_message(&self, content: &str) -> Result<Message> {
        self.http.send_message(
            self.channel_id,
            &json!({ "content": content }),
        ).await.map_err(|e| e.into())
    }
    
    pub async fn reply_to(&self, reply_to: MessageId, content: &str) -> Result<Message> {
        self.http.send_message(
            self.channel_id,
            &json!({
                "content": content,
                "message_reference": {
                    "message_id": reply_to
                }
            }),
        ).await.map_err(|e| e.into())
    }
}
```

### 4.3 渲染进度条

```rust
fn render_progress_bar(progress: f32) -> String {
    let filled = (progress / 10.0).round() as usize;
    let empty = 10 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

// 示例输出: [██████░░░░] 60%
```

## 5. 通信最佳实践

### 5.1 Task ID 使用规范

- ✅ **必须包含**: 每条任务相关消息都应有 task_id
- ✅ **显式声明**: 使用 #short-id 格式，如 #a7b3c9d2
- ✅ **一致性**: 同一任务的所有消息使用相同的 task_id
- ✅ **清晰可读**: 消息中 task_id 前后应有空格或标点

### 5.2 消息简洁性

- 使用 Markdown 格式提高可读性
- 关键信息放在前面
- 使用列表和代码块组织信息
- 避免过长的消息（超过 2000 字符应考虑分段）

### 5.3 回复时效性

- 认领任务后应立即回复确认
- 收到心跳询问后应在 5 分钟内回复
- 进度更新应根据任务时长合理安排频率

### 5.4 错误处理

- 明确说明错误原因
- 提供是否可以重试的信息
- 给出替代方案或建议
- 保持专业和建设性的语气

## 6. 示例对话流

### 6.1 正常任务执行

```
[09:00] User: @a 分析代码库
[09:00] Agent A: 分析中，拆解任务...
[09:01] Agent A: @agent-b 新任务 #a7b3c9d2：读取文件结构...
[09:01] Agent A: @agent-c 新任务 #f8e2d1a3：分析模块依赖...
[09:02] Agent B: @agent-a 已认领任务 #a7b3c9d2
[09:03] Agent C: @agent-a 已认领任务 #f8e2d1a3
[09:08] Agent B: @agent-a 任务 #a7b3c9d2 进度更新 [██░░░░░░░░] 20%
[09:10] Agent C: @agent-a 任务 #f8e2d1a3 进度更新 [████░░░░░░] 40%
[09:15] Agent B: @agent-a 任务 #a7b3c9d2 已完成 ✅
[09:18] Agent C: @agent-a 任务 #f8e2d1a3 已完成 ✅
[09:19] Agent A: 分析完成！发现 15 个模块...
```

### 6.2 停滞检测与处理

```
[09:30] Agent A: @agent-c 任务 #f8e2d1a3 已超过 15 分钟无进度更新...
[09:32] Agent C: @agent-a 任务 #f8e2d1a3 仍在执行，遇到复杂依赖，预计还需 10 分钟
[09:35] Agent C: @agent-a 任务 #f8e2d1a3 进度更新 [███████░░░] 70%
[09:40] Agent C: @agent-a 任务 #f8e2d1a3 已完成 ✅
```

### 6.3 任务重新分配

```
[09:30] Agent A: @agent-c 任务 #f8e2d1a3 已超过 15 分钟无进度更新...
[09:45] Agent A: @agent-c 任务 #f8e2d1a3 询问超时，将重新分配
[09:45] Agent A: @agent-b 请接手任务 #f8e2d1a3，当前进度 40%...
[09:46] Agent B: @agent-a 已接手任务 #f8e2d1a3
[09:55] Agent B: @agent-a 任务 #f8e2d1a3 已完成 ✅
```

## 7. 相关文档

- [Agent 定义系统](agent-definition.md) - 如何编写 SOUL.md 和 TOOL.md
- [任务系统](task-system.md) - Task ID 规范和任务生命周期
- [工具系统](tools.md) - send_notification 等工具的使用
- [心跳机制](heartbeat.md) - 停滞检测和询问逻辑
- [工作流程](workflows.md) - 完整的交互流程
