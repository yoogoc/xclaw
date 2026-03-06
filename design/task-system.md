# 任务系统

## 1. Task ID 规范

### 1.1 ID 格式

```rust
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    /// 短格式（前8位）用于显示
    pub fn short(&self) -> String {
        self.0.to_string()[..8].to_string()
    }
    
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}
```

- **完整 ID**: `a7b3c9d2-e4f5-6789-abcd-ef0123456789`
- **短 ID**: `a7b3c9d2`（用于 Discord 消息显示）
- **生成时机**: task_create 时生成，全程唯一且不变

### 1.2 ID 使用规范

| 场景 | 使用格式 | 示例 |
|------|----------|------|
| 工具调用 | 短 ID 或完整 ID | `task_id: "a7b3c9d2"` |
| Discord 消息 | 短 ID 带 # 前缀 | `#a7b3c9d2` |
| 文件存储 | 完整 ID | `a7b3c9d2-e4f5-...json` |
| 日志记录 | 短 ID | `任务 a7b3c9d2 已完成` |

## 2. 任务数据结构

### 2.1 Task 结构定义

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub task_id: TaskId,
    pub short_id: String,
    pub title: String,
    pub description: String,
    pub created_by: String,           // 创建者 agent_id
    pub assigned_to: Option<String>,  // 分配给哪个 agent
    pub status: TaskStatus,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub deadline: Option<DateTime<Utc>>,
    pub progress: f32,                // 0.0 - 100.0
    pub current_step: Option<String>,
    pub parameters: Value,            // 任务参数(JSON)
    pub result: Option<TaskResult>,
    pub logs: Vec<LogEntry>,
    pub parent_task: Option<TaskId>,  // 父任务ID
    pub dependencies: Vec<TaskId>,    // 依赖的其他任务
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,      // 待认领
    Active,       // 执行中
    Completed,    // 已完成
    Failed,       // 失败
    Cancelled,    // 已取消
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskResult {
    pub success: bool,
    pub output: Value,
    pub artifacts: Vec<String>,
    pub execution_time_seconds: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub agent: String,
    pub action: String,
    pub message: String,
}
```

### 2.2 任务 JSON 文件示例

```json
{
  "task_id": "a7b3c9d2-e4f5-6789-abcd-ef0123456789",
  "short_id": "a7b3c9d2",
  "title": "读取项目文件结构",
  "description": "分析代码库的文件组织，识别主要模块和入口文件",
  "created_by": "agent-a",
  "assigned_to": "agent-b",
  "status": "active",
  "priority": "high",
  "created_at": "2024-01-15T09:00:00Z",
  "updated_at": "2024-01-15T09:15:00Z",
  "started_at": "2024-01-15T09:05:00Z",
  "completed_at": null,
  "deadline": "2024-01-15T10:00:00Z",
  "progress": 50.0,
  "current_step": "扫描 src 目录",
  "parameters": {
    "target_path": "./src",
    "depth": 3,
    "include_hidden": false
  },
  "result": null,
  "logs": [
    {
      "timestamp": "2024-01-15T09:00:00Z",
      "agent": "agent-a",
      "action": "created",
      "message": "创建任务并分配给 agent-b"
    },
    {
      "timestamp": "2024-01-15T09:05:00Z",
      "agent": "agent-b",
      "action": "claimed",
      "message": "认领任务，开始执行"
    },
    {
      "timestamp": "2024-01-15T09:10:00Z",
      "agent": "agent-b",
      "action": "progress",
      "message": "进度 25% - 扫描根目录完成"
    },
    {
      "timestamp": "2024-01-15T09:15:00Z",
      "agent": "agent-b",
      "action": "progress",
      "message": "进度 50% - 扫描 src 目录"
    }
  ],
  "parent_task": null,
  "dependencies": []
}
```

## 3. 任务存储结构

### 3.1 目录布局

```
workspace/
├── tasks/
│   ├── pending/          # 待认领
│   │   └── a7b3c9d2-e4f5-6789-abcd-ef0123456789.json
│   ├── active/           # 执行中
│   │   └── f8e2d1a3-e4f5-6789-abcd-ef0123456789.json
│   ├── completed/        # 已完成
│   │   └── c3d4e5f6-e4f5-6789-abcd-ef0123456789.json
│   └── failed/           # 失败
│       └── b2a3c4d5-e4f5-6789-abcd-ef0123456789.json
```

### 3.2 文件命名规则

- 使用完整 task_id 作为文件名
- 扩展名 `.json`
- 示例：`a7b3c9d2-e4f5-6789-abcd-ef0123456789.json`

### 3.3 状态流转与文件移动

```
Pending (pending/) 
  │
  │ claim
  ▼
Active (active/)
  │
  ├─ complete ──► Completed (completed/)
  │
  ├─ fail ──────► Failed (failed/)
  │
  └─ cancel ────► Cancelled (completed/)
```

## 4. 任务状态机

### 4.1 状态定义

| 状态 | 说明 | 转移条件 |
|------|------|----------|
| Pending | 已创建，等待认领 | task_create 后 |
| Active | 已认领，执行中 | task_claim 后 |
| Completed | 已完成 | task_complete 后 |
| Failed | 执行失败 | task_fail 后 |
| Cancelled | 已取消 | 超时或手动取消 |

### 4.2 状态转移图

```
                    ┌─────────────┐
                    │   Pending   │
                    └──────┬──────┘
                           │ claim
                           ▼
┌──────────┐      ┌─────────────┐      ┌───────────┐
│ Failed   │◄─────│    Active   │─────►│ Completed │
└──────────┘ fail  └──────┬──────┘complete└───────────┘
                          │
                          │ cancel
                          ▼
                   ┌─────────────┐
                   │  Cancelled  │
                   └─────────────┘
```

## 5. TaskStore 实现

### 5.1 核心接口

```rust
pub struct TaskStore {
    workspace_path: PathBuf,
}

impl TaskStore {
    /// 创建任务
    pub async fn create(&self, task: Task) -> Result<TaskId> {
        let task_path = self.workspace_path
            .join("tasks")
            .join("pending")
            .join(format!("{}.json", task.task_id));
        
        fs::write(&task_path, serde_json::to_string_pretty(&task)?).await?;
        Ok(task.task_id)
    }
    
    /// 读取任务（支持短 ID）
    pub async fn read(&self, task_id: TaskId) -> Result<Task> {
        // 在所有目录中查找
        for dir in ["pending", "active", "completed", "failed"] {
            let path = self.workspace_path
                .join("tasks")
                .join(dir)
                .join(format!("{}.json", task_id));
            
            if path.exists() {
                let content = fs::read_to_string(&path).await?;
                return Ok(serde_json::from_str(&content)?);
            }
        }
        
        Err(Error::TaskNotFound(task_id))
    }
    
    /// 认领任务
    pub async fn claim(&self, task_id: TaskId, agent_id: &str) -> Result<Task> {
        let from_path = self.workspace_path
            .join("tasks")
            .join("pending")
            .join(format!("{}.json", task_id));
        
        let to_path = self.workspace_path
            .join("tasks")
            .join("active")
            .join(format!("{}.json", task_id));
        
        let content = fs::read_to_string(&from_path).await?;
        let mut task: Task = serde_json::from_str(&content)?;
        
        // 更新状态
        task.assigned_to = Some(agent_id.to_string());
        task.status = TaskStatus::Active;
        task.started_at = Some(Utc::now());
        task.updated_at = Utc::now();
        task.logs.push(LogEntry {
            timestamp: Utc::now(),
            agent: agent_id.to_string(),
            action: "claimed".to_string(),
            message: format!("{} 认领了任务", agent_id),
        });
        
        // 移动文件
        fs::write(&to_path, serde_json::to_string_pretty(&task)?).await?;
        fs::remove_file(&from_path).await?;
        
        Ok(task)
    }
    
    /// 更新进度
    pub async fn update_progress(
        &self,
        task_id: TaskId,
        progress: f32,
        step: &str,
    ) -> Result<Task> {
        let path = self.workspace_path
            .join("tasks")
            .join("active")
            .join(format!("{}.json", task_id));
        
        let content = fs::read_to_string(&path).await?;
        let mut task: Task = serde_json::from_str(&content)?;
        
        task.progress = progress;
        task.current_step = Some(step.to_string());
        task.updated_at = Utc::now();
        task.logs.push(LogEntry {
            timestamp: Utc::now(),
            agent: task.assigned_to.clone().unwrap_or_default(),
            action: "progress".to_string(),
            message: format!("进度 {}% - {}", progress, step),
        });
        
        fs::write(&path, serde_json::to_string_pretty(&task)?).await?;
        
        Ok(task)
    }
    
    /// 完成任务
    pub async fn complete(&self, task_id: TaskId, result: TaskResult) -> Result<Task> {
        let from_path = self.workspace_path
            .join("tasks")
            .join("active")
            .join(format!("{}.json", task_id));
        
        let to_path = self.workspace_path
            .join("tasks")
            .join("completed")
            .join(format!("{}.json", task_id));
        
        let content = fs::read_to_string(&from_path).await?;
        let mut task: Task = serde_json::from_str(&content)?;
        
        task.status = TaskStatus::Completed;
        task.progress = 100.0;
        task.completed_at = Some(Utc::now());
        task.updated_at = Utc::now();
        task.result = Some(result);
        task.logs.push(LogEntry {
            timestamp: Utc::now(),
            agent: task.assigned_to.clone().unwrap_or_default(),
            action: "completed".to_string(),
            message: "任务完成".to_string(),
        });
        
        fs::write(&to_path, serde_json::to_string_pretty(&task)?).await?;
        fs::remove_file(&from_path).await?;
        
        Ok(task)
    }
    
    /// 列出任务
    pub async fn list(&self, status: Option<TaskStatus>) -> Result<Vec<Task>> {
        let mut tasks = vec![];
        
        let dirs = match status {
            Some(TaskStatus::Pending) => vec!["pending"],
            Some(TaskStatus::Active) => vec!["active"],
            Some(TaskStatus::Completed) => vec!["completed"],
            Some(TaskStatus::Failed) => vec!["failed"],
            _ => vec!["pending", "active", "completed", "failed"],
        };
        
        for dir in dirs {
            let path = self.workspace_path.join("tasks").join(dir);
            if !path.exists() {
                continue;
            }
            
            let mut entries = fs::read_dir(&path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let content = fs::read_to_string(entry.path()).await?;
                let task: Task = serde_json::from_str(&content)?;
                tasks.push(task);
            }
        }
        
        Ok(tasks)
    }
}
```

## 6. 任务查询与过滤

### 6.1 常用查询场景

```rust
// 列出分配给特定 Agent 的待办任务
pub async fn list_pending_for_agent(&self, agent_id: &str) -> Result<Vec<Task>> {
    let all = self.list(Some(TaskStatus::Pending)).await?;
    Ok(all.into_iter()
        .filter(|t| t.assigned_to.as_ref().map(|a| a == agent_id).unwrap_or(false))
        .collect())
}

// 列出所有活跃任务（用于心跳检查）
pub async fn list_active(&self) -> Result<Vec<Task>> {
    self.list(Some(TaskStatus::Active)).await
}

// 检查任务是否停滞
pub async fn is_stale(&self, task_id: &TaskId, threshold: Duration) -> Result<bool> {
    let task = self.read(*task_id).await?;
    let elapsed = Utc::now() - task.updated_at;
    Ok(elapsed > threshold)
}
```

## 7. 错误处理

### 7.1 任务系统错误类型

```rust
pub enum TaskError {
    NotFound(TaskId),
    AlreadyClaimed(TaskId, String),  // task_id, claimed_by
    InvalidTransition(TaskStatus, TaskStatus),
    PermissionDenied(String),
    ValidationError(String),
}

impl std::fmt::Display for TaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskError::NotFound(id) => write!(f, "任务 {} 不存在", id.short()),
            TaskError::AlreadyClaimed(id, by) => {
                write!(f, "任务 {} 已被 {} 认领", id.short(), by)
            }
            TaskError::InvalidTransition(from, to) => {
                write!(f, "无效的状态转移: {:?} -> {:?}", from, to)
            }
            TaskError::PermissionDenied(msg) => write!(f, "权限不足: {}", msg),
            TaskError::ValidationError(msg) => write!(f, "验证失败: {}", msg),
        }
    }
}
```

## 8. 最佳实践

### 8.1 Task ID 使用

1. **显示使用短 ID**: 人类可读，便于沟通
2. **存储使用完整 ID**: 避免冲突
3. **日志记录短 ID**: 简洁清晰
4. **工具调用支持两种格式**: 提高灵活性

### 8.2 任务粒度

- 每个任务应该是**可独立完成**的单元
- 任务执行时间建议在 **5-30 分钟** 之间
- 过大的任务应该拆解为子任务
- 使用 parent_task 字段关联父子任务

### 8.3 进度更新频率

- 短任务（<10分钟）: 完成时更新一次
- 中等任务（10-30分钟）: 每 5-10 分钟更新
- 长任务（>30分钟）: 每 10-15 分钟更新
- 遇到重要里程碑时立即更新
