# 5. 工具系统

## 5.1 核心工具清单

| 工具 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `task_create` | title, description, assign_to, priority?, parameters? | {task_id, short_id} | 创建新任务 |
| `task_read` | task_id | Task 对象 | 读取任务详情 |
| `task_claim` | task_id | {success, task} | 认领任务 |
| `task_progress` | task_id, progress, step | {success} | 更新进度 |
| `task_complete` | task_id, result, artifacts? | {success} | 完成任务 |
| `task_list` | status?, assignee? | Task[] | 列出任务 |
| `send_notification` | to, message, task_id? | {success} | 发送 Discord 通知 |

## 5.2 工具详细说明

### task_create

创建新任务到共享 workspace。

**参数说明**:
- `title` (string, required): 任务标题
- `description` (string, required): 任务详细描述
- `assign_to` (string, required): 分配给哪个 agent（如 "agent-b"）
- `priority` (enum, optional): 优先级，low/medium/high，默认 medium
- `parameters` (object, optional): 任务参数，JSON 对象

**返回示例**:
```json
{
  "task_id": "a7b3c9d2-e4f5-6789-abcd-ef0123456789",
  "short_id": "a7b3c9d2",
  "message": "任务已创建"
}
```

**使用示例**:
```
task_create
  title: "读取文件结构"
  description: "分析代码库的目录组织和文件分布"
  assign_to: "agent-b"
  priority: high
  parameters: { "target_path": "./src", "depth": 3 }
```

**实现要点**:
1. 生成 UUID 作为 task_id
2. 提取前 8 位作为 short_id
3. 写入 workspace/tasks/pending/{task_id}.json
4. 设置初始状态为 pending

---

### task_read

读取指定任务的详细信息。

**参数说明**:
- `task_id` (string, required): 任务 ID，可以是 8 位短 ID 或完整 UUID

**返回示例**:
```json
{
  "task_id": "a7b3c9d2-e4f5-6789-abcd-ef0123456789",
  "short_id": "a7b3c9d2",
  "title": "读取文件结构",
  "description": "...",
  "created_by": "agent-a",
  "assigned_to": "agent-b",
  "status": "active",
  "progress": 50.0,
  "current_step": "扫描 src 目录",
  "parameters": { "target_path": "./src" },
  "created_at": "2024-01-15T09:00:00Z",
  "updated_at": "2024-01-15T09:15:00Z"
}
```

**使用示例**:
```
task_read task_id: "a7b3c9d2"
```

---

### task_claim

认领一个待处理任务。

**参数说明**:
- `task_id` (string, required): 任务 ID

**返回示例**:
```json
{
  "success": true,
  "task": {
    "task_id": "a7b3c9d2-e4f5-6789-abcd-ef0123456789",
    "status": "active",
    "assigned_to": "agent-b",
    "started_at": "2024-01-15T09:05:00Z"
  }
}
```

**使用场景**:
收到任务通知后，调用此工具认领任务，表示你将负责执行。

**使用示例**:
```
task_claim task_id: "a7b3c9d2"
```

**实现要点**:
1. 从 pending 目录移动到 active 目录
2. 更新 status 为 active
3. 设置 assigned_to 和 started_at

---

### task_progress

更新任务执行进度。

**参数说明**:
- `task_id` (string, required): 任务 ID
- `progress` (number, required): 进度百分比，0-100
- `step` (string, required): 当前执行步骤描述
- `message` (string, optional): 附加消息

**返回示例**:
```json
{
  "success": true,
  "task_id": "a7b3c9d2",
  "progress": 50,
  "updated_at": "2024-01-15T09:15:00Z"
}
```

**使用示例**:
```
task_progress
  task_id: "a7b3c9d2"
  progress: 50
  step: "扫描 src 目录完成"
  message: "已识别 15 个模块"
```

**重要提示**:
- 执行过程中定期调用，建议每完成一个重要步骤更新一次
- 更新会自动触发心跳时间戳重置

---

### task_complete

标记任务完成并提交结果。

**参数说明**:
- `task_id` (string, required): 任务 ID
- `result` (object, required): 任务结果数据
- `artifacts` (array, optional): 产物文件路径列表

**返回示例**:
```json
{
  "success": true,
  "task_id": "a7b3c9d2",
  "status": "completed",
  "completed_at": "2024-01-15T09:30:00Z"
}
```

**使用示例**:
```
task_complete
  task_id: "a7b3c9d2"
  result: {
    "files_found": 42,
    "main_modules": ["src/core", "src/agent", "src/tools"],
    "analysis_summary": "..."
  }
  artifacts: ["workspace/artifacts/file_tree.json"]
```

**实现要点**:
1. 从 active 目录移动到 completed 目录
2. 设置 status 为 completed
3. 设置 completed_at 时间戳
4. 保存 result 数据

---

### task_list

列出任务。

**参数说明**:
- `status` (enum, optional): 状态筛选，pending/active/completed/failed/all，默认 all
- `assignee` (string, optional): 筛选分配给指定 agent 的任务

**返回示例**:
```json
{
  "tasks": [
    {
      "task_id": "a7b3c9d2",
      "title": "读取文件结构",
      "status": "active",
      "progress": 50,
      "assigned_to": "agent-b",
      "updated_at": "2024-01-15T09:15:00Z"
    },
    {
      "task_id": "f8e2d1a3",
      "title": "分析模块依赖",
      "status": "pending",
      "progress": 0,
      "assigned_to": "agent-c",
      "updated_at": "2024-01-15T09:00:00Z"
    }
  ],
  "count": 2
}
```

**使用示例**:
```
task_list status: "active"
task_list status: "pending" assignee: "agent-b"
```

---

### send_notification

发送 Discord 通知给其他 Agent。

**参数说明**:
- `to` (string, required): 接收者 agent ID（如 "agent-b"）
- `message` (string, required): 消息内容
- `task_id` (string, optional 但强烈建议提供): 关联的任务 ID

**返回示例**:
```json
{
  "success": true,
  "message_id": "123456789"
}
```

**使用示例**:
```
# 分配任务
send_notification
  to: "agent-b"
  message: "@agent-b 新任务 #a7b3c9d2：读取项目文件结构"
  task_id: "a7b3c9d2"

# 进度更新
send_notification
  to: "agent-a"
  message: "@a 任务 #a7b3c9d2 进度 50% - 扫描 src 目录完成"
  task_id: "a7b3c9d2"

# 任务完成
send_notification
  to: "agent-a"
  message: "@a 任务 #a7b3c9d2 已完成 ✅"
  task_id: "a7b3c9d2"
```

**重要提示**:
- 分配任务或回复进度时，**必须**在 message 中包含 task_id
- 消息会被发送到 Discord，@mention 会通知对应 Agent

## 5.3 工具实现框架

```rust
// src/tools/mod.rs

use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, params: Value) -> Result<Value, ToolError>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }
    
    pub async fn execute(&self, name: &str, params: Value) -> Result<Value, ToolError> {
        let tool = self.tools.get(name)
            .ok_or(ToolError::NotFound(name.to_string()))?;
        tool.execute(params).await
    }
    
    pub fn descriptions(&self) -> String {
        // 生成 TOOL.md 格式的工具说明
        self.tools.values()
            .map(|t| format!("## {}\n{}\n", t.name(), t.description()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
```

## 5.4 错误处理

所有工具统一返回 Result，错误类型：

```rust
pub enum ToolError {
    NotFound(String),           // 工具不存在
    MissingParam(String),       // 缺少必需参数
    InvalidParam(String),       // 参数格式错误
    TaskNotFound(TaskId),       // 任务不存在
    TaskAlreadyClaimed,         // 任务已被认领
    Unauthorized,               // 无权操作
    WorkspaceError(String),     // 文件系统错误
}
```

错误会被转换为 JSON 返回给 LLM：
```json
{
  "success": false,
  "error": "TaskNotFound",
  "message": "任务 a7b3c9d2 不存在"
}
```
