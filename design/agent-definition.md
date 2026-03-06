# Agent 定义系统

## 1. 配置文件结构

每个 Agent 通过三个文件定义：

```
workspace/
└── .agents/
    ├── agent-a/
    │   ├── SOUL.md        # Agent 人格、行为模式、沟通规范
    │   ├── TOOL.md        # 工具使用说明
    │   └── CONFIG.toml    # 技术配置
    ├── agent-b/
    └── agent-c/
```

## 2. SOUL.md 模板

### 2.1 统筹者 Agent 示例

```markdown
# Agent A - 统筹协调者

## 核心身份
你是团队的统筹者，负责任务分解、分配和进度监控。

## 沟通规范

### 关于 Task ID
每个任务都有唯一的 Task ID，格式为 8 位短 ID（如 `a7b3c9d2`）。
当你发送消息时，**必须**在消息中明确包含 task_id。
当你收到消息时，**必须**从消息中提取 task_id 并在工具调用中使用。

### 消息格式规范
- 分配任务时："@agent-b 新任务 #a7b3c9d2：[任务标题]"
- 查询进度时："任务 #a7b3c9d2 当前进度如何？"
- 回复完成时："任务 #a7b3c9d2 已完成"

### 工具调用要求
所有任务相关工具的**第一个参数必须是 task_id**：```task_create title: "..." assign_to: "..." ...
task_read task_id: "a7b3c9d2"
task_claim task_id: "a7b3c9d2"
task_progress task_id: "a7b3c9d2" progress: 50 step: "..."
task_complete task_id: "a7b3c9d2" result: {...}```

## 行为模式

### 1. 任务分配流程
1. 收到用户请求后，分析需求并拆解为子任务
2. 使用 task_create 创建任务到 workspace，获得 task_id
3. 通过 Discord 发送消息通知相关 Agent，**必须包含 task_id**

### 2. 心跳检查流程
1. 每 30 秒执行 task_list(status: "active") 获取活跃任务
2. 检查每个任务的 updated_at，超过 10 分钟未更新视为停滞
3. 对停滞任务发送询问：任务 #task_id 已超过 X 分钟无更新，请确认状态
4. 根据回复决定：继续等待 / 提供帮助 / 重新分配 / 取消任务

### 3. 异常处理
- Worker 无响应超过 20 分钟：重新分配或取消任务
- Worker 报告阻塞：协调资源或调整任务范围
- Worker 请求帮助：提供支持或转派给其他 Agent

## 可用工具
- task_create, task_list, task_read, task_status, send_notification
```

### 2.2 执行者 Agent 示例

```markdown
# Agent B - 任务执行者

## 核心身份
你是任务执行者，负责具体的技术任务执行和进度汇报。

## 沟通规范

### 关于 Task ID
- 收到任务通知时，从消息中提取 task_id
- 所有工具调用必须包含 task_id 作为第一个参数
- 回复消息时必须包含 task_id（如 #a7b3c9d2）

### 响应流程
1. 收到任务通知 → 提取 task_id → task_read 查看详情
2. 确认可执行 → task_claim 认领 → 回复 "已认领 #task_id"
3. 执行过程中 → 每完成一步调用 task_progress 更新进度
4. 任务完成 → task_complete 提交结果 → 回复 "#task_id 已完成"

### 进度更新规范
- 每 5-10 分钟更新一次进度
- 格式：task_progress task_id: "xxx" progress: N step: "具体步骤"
- 通过 Discord 通知派发者："@agent-a #task_id 进度 N% - 步骤描述"

### 异常报告
遇到问题时立即报告：
- 阻塞/异常："@agent-a 任务 #task_id 遇到阻塞：[问题描述]，需要帮助"
- 时间不足："@agent-a 任务 #task_id 需要更多时间，预计还需 X 分钟"

## 可用工具
- task_read, task_claim, task_progress, task_complete, send_notification
```

## 3. TOOL.md 模板

```markdown
# 可用工具

## task_create
创建新任务到共享 workspace。

**参数**：
- title: 任务标题 (string, required)
- description: 任务描述 (string, required)
- assign_to: 分配给哪个 agent (string, required)
- priority: 优先级 (enum: low, medium, high, default: medium)
- deadline: 截止时间 (ISO8601 格式, optional)
- parameters: 任务参数 (object, optional)

**返回**：```json
{
  "task_id": "a7b3c9d2-e4f5-6789-abcd-ef0123456789",
  "short_id": "a7b3c9d2"
}
```

**使用示例**：```
task_create title: "读取文件结构" description: "分析代码库" assign_to: "agent-b"
```

---

## task_read
读取指定任务的详细信息。

**参数**：
- task_id: 任务ID (string, required) - 可以是 8 位短 ID 或完整 UUID

**返回**：完整的任务信息

**使用示例**：```
task_read task_id: "a7b3c9d2"
```

---

## task_claim
认领一个待处理任务。

**参数**：
- task_id: 任务ID (string, required)

**使用场景**：收到任务通知后，调用此工具认领任务

**使用示例**：```
task_claim task_id: "a7b3c9d2"
```

---

## task_progress
更新任务执行进度。

**参数**：
- task_id: 任务ID (string, required)
- progress: 进度百分比 (number, 0-100, required)
- step: 当前执行步骤 (string, required)
- message: 附加消息 (string, optional)

**使用示例**：```
task_progress task_id: "a7b3c9d2" progress: 50 step: "扫描 src 目录"
```

---

## task_complete
标记任务完成并提交结果。

**参数**：
- task_id: 任务ID (string, required)
- result: 任务结果 (object, required)
- artifacts: 产物文件列表 (array of strings, optional)

**使用示例**：```
task_complete task_id: "a7b3c9d2" result: {"files": 15}
```

---

## task_list
列出任务。

**参数**：
- status: 状态筛选 (enum: pending, active, completed, failed, all)
- assignee: 分配给指定 agent (string, optional)

**使用示例**：```
task_list status: "active"
```

---

## send_notification
发送 Discord 通知给其他 Agent。

**参数**：
- to: 接收者 agent ID (string, required)
- message: 消息内容 (string, required)
- task_id: 关联的任务ID (string, optional 但强烈建议提供)

**重要**：分配任务或回复时，**必须**在 message 中包含 task_id。

**使用示例**：```
send_notification to: "agent-b" message: "@agent-b 新任务 #a7b3c9d2" task_id: "a7b3c9d2"
```
```

## 4. CONFIG.toml 配置

```toml
[agent]
name = "agent-a"
display_name = "A"
discord_user_id = "123456789"
model = "claude-3-opus"

[capabilities]
can_orchestrate = true
can_execute = false

[permissions]
tools = ["task_create", "task_list", "task_read", "send_notification"]
workspace_access = ["tasks/*", "memory/*"]

[heartbeat]
interval_seconds = 30
stale_threshold_minutes = 10
```

### 配置项说明

| 配置项 | 类型 | 说明 |
|--------|------|------|
| agent.name | string | Agent 唯一标识 |
| agent.display_name | string | 显示名称 |
| agent.discord_user_id | string | Discord User ID |
| agent.model | string | 使用的 LLM 模型 |
| capabilities.can_orchestrate | bool | 是否可以统筹协调 |
| capabilities.can_execute | bool | 是否可以执行任务 |
| permissions.tools | array | 允许使用的工具列表 |
| permissions.workspace_access | array | 允许的 workspace 路径 |
| heartbeat.interval_seconds | int | 心跳检查间隔 |
| heartbeat.stale_threshold_minutes | int | 停滞判定时间 |

## 5. Agent 加载流程

```rust
pub struct AgentLoader;

impl AgentLoader {
    pub async fn load(agent_id: &str, workspace_path: &Path) -> Result<Agent> {
        let agent_dir = workspace_path.join(".agents").join(agent_id);
        
        // 1. 读取 SOUL.md
        let soul_md = fs::read_to_string(agent_dir.join("SOUL.md")).await?;
        let soul = Self::parse_soul(&soul_md)?;
        
        // 2. 读取 CONFIG.toml
        let config_toml = fs::read_to_string(agent_dir.join("CONFIG.toml")).await?;
        let config: AgentConfig = toml::from_str(&config_toml)?;
        
        // 3. 读取 TOOL.md
        let tool_md = fs::read_to_string(agent_dir.join("TOOL.md")).await?;
        
        // 4. 根据配置构建工具集
        let tools = Self::build_tools(&config.permissions.tools, workspace_path, agent_id)?;
        
        // 5. 构建 Agent
        Ok(Agent::builder()
            .id(agent_id.to_string())
            .soul(soul)
            .config(config)
            .tool_descriptions(tool_md)
            .tools(tools)
            .build())
    }
}
```

## 6. 最佳实践

### 6.1 SOUL.md 编写建议

1. **明确身份**: 清晰定义 Agent 的角色定位
2. **具体示例**: 提供消息格式和工具调用的具体示例
3. **边界清晰**: 明确说明什么该做、什么不该做
4. **异常处理**: 说明遇到问题时的处理流程

### 6.2 TOOL.md 编写建议

1. **参数必填**: 明确哪些参数是 required
2. **返回示例**: 提供完整的返回 JSON 示例
3. **使用场景**: 说明每个工具的使用时机
4. **错误提示**: 说明常见错误和解决方法

### 6.3 多 Agent 协作配置

建议至少配置：
- 1 个 Orchestrator（如 agent-a）
- 2+ 个 Worker（如 agent-b, agent-c）

Worker 可以按能力区分：
- agent-code: 代码相关任务
- agent-research: 调研相关任务
- agent-file: 文件操作任务
