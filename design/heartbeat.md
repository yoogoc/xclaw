# 6. 心跳机制

## 6.1 概述

心跳机制用于监控活跃任务的执行状态，及时发现停滞任务并进行干预。

**核心职责**:
- 定期检查所有活跃任务
- 识别长时间无更新的任务
- 主动询问执行者状态
- 根据反馈决定后续处理

## 6.2 监控参数

| 参数 | 默认值 | 说明 |
|------|--------|------|
| 检查间隔 | 30 秒 | 每次心跳检查的时间间隔 |
| 停滞阈值 | 10 分钟 | 超过此时间无更新视为停滞 |
| 询问超时 | 20 分钟 | 询问后超过此时间无响应视为失联 |

## 6.3 核心实现

```rust
// src/agent/heartbeat.rs

pub struct HeartbeatMonitor {
    workspace: Arc<Workspace>,
    interval: Duration,
    stale_threshold: Duration,
    query_timeout: Duration,
    pending_queries: HashMap<TaskId, DateTime<Utc>>,
}

impl HeartbeatMonitor {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self {
            workspace,
            interval: Duration::from_secs(30),
            stale_threshold: Duration::from_secs(600), // 10 分钟
            query_timeout: Duration::from_secs(1200),  // 20 分钟
            pending_queries: HashMap::new(),
        }
    }
    
    /// 启动心跳监控循环
    pub async fn run(mut self, notification: Arc<dyn Notification>) {
        let mut interval = tokio::time::interval(self.interval);
        
        loop {
            interval.tick().await;
            
            // 1. 获取所有活跃任务
            let active_tasks = match self.workspace.list_active().await {
                Ok(tasks) => tasks,
                Err(e) => {
                    error!("获取活跃任务失败: {}", e);
                    continue;
                }
            };
            
            // 2. 检查每个任务
            for task in active_tasks {
                self.check_task(task, notification.clone()).await;
            }
            
            // 3. 清理超时的查询记录
            self.cleanup_expired_queries();
        }
    }
    
    /// 检查单个任务
    async fn check_task(
        &mut self,
        task: Task,
        notification: Arc<dyn Notification>
    ) {
        let now = Utc::now();
        let elapsed = now - task.updated_at;
        
        // 情况 1: 正常执行中
        if elapsed < self.stale_threshold {
            debug!("任务 {} 正常，上次更新 {} 秒前", task.task_id, elapsed.num_seconds());
            return;
        }
        
        // 情况 2: 已有待处理的询问
        if let Some(query_time) = self.pending_queries.get(&task.task_id) {
            let query_elapsed = now - *query_time;
            
            // 询问超时（超过 20 分钟无响应）
            if query_elapsed > self.query_timeout {
                warn!("任务 {} 询问超时，Agent {} 无响应", task.task_id, task.assigned_to);
                self.handle_agent_unresponsive(&task, notification).await;
                self.pending_queries.remove(&task.task_id);
            }
            return;
        }
        
        // 情况 3: 停滞，需要询问
        warn!("任务 {} 停滞，上次更新 {} 分钟前", task.task_id, elapsed.num_minutes());
        self.query_agent_status(&task, notification).await;
        self.pending_queries.insert(task.task_id, now);
    }
    
    /// 询问 Agent 状态
    async fn query_agent_status(&self, task: &Task, notification: Arc<dyn Notification>) {
        let elapsed_minutes = (Utc::now() - task.updated_at).num_minutes();
        
        let message = format!(
            "@{} 任务 #{} 已超过 {} 分钟无进度更新，请确认：\n\
            1. 是否仍在执行？\n\
            2. 是否遇到阻塞需要协助？\n\
            3. 预计何时完成？",
            task.assigned_to,
            task.task_id.short(),
            elapsed_minutes
        );
        
        notification.send(&task.assigned_to, &message, Some(&task.task_id)).await;
        
        info!("已向 {} 发送关于任务 {} 的状态询问", task.assigned_to, task.task_id);
    }
    
    /// 处理 Agent 无响应
    async fn handle_agent_unresponsive(
        &self,
        task: &Task,
        notification: Arc<dyn Notification>
    ) {
        // 策略 1: 通知统筹者
        notification.send(
            &task.created_by,
            &format!("@{} Agent {} 执行任务 #{} 时失联（超过 20 分钟无响应），建议重新分配",
                task.created_by,
                task.assigned_to,
                task.task_id.short()
            ),
            Some(&task.task_id)
        ).await;
        
        // 策略 2: 标记任务为异常（可选）
        // self.workspace.mark_task_abnormal(&task.task_id).await;
    }
    
    fn cleanup_expired_queries(&mut self) {
        let now = Utc::now();
        self.pending_queries.retain(|_, query_time| {
            now - *query_time < self.query_timeout
        });
    }
}
```

## 6.4 询问后的处理流程

### 6.4.1 Agent 回复正常继续

```
Agent C 收到询问：
  "@agent-c 任务 #f8e2d1a3 已超过 15 分钟无进度更新..."

Agent C 响应：
  send_notification(
    to: "agent-a",
    message: "@a 任务 #f8e2d1a3 仍在执行，正在处理复杂的循环依赖分析，
             预计还需 10 分钟完成，当前实际进度约 40%",
    task_id: "f8e2d1a3"
  )
  
  task_progress(
    task_id: "f8e2d1a3",
    progress: 40,
    step: "解析循环依赖"
  )

Agent A 收到回复后：
  - 从 pending_queries 中移除该任务
  - 重置监控状态
  - 记录新的 ETA（如果需要）
```

### 6.4.2 Agent 回复需要帮助

```
Agent C 响应：
  send_notification(
    to: "agent-a",
    message: "@a 任务 #f8e2d1a3 遇到阻塞：无法访问私有仓库 npm.example.com，
             需要配置 token 或调整任务范围",
    task_id: "f8e2d1a3"
  )

Agent A 处理：
  - 从 pending_queries 中移除
  - 分析阻塞原因
  
  决策 1 - 重新分配：
    send_notification(
      to: "agent-b",
      message: "@agent-b agent-c 在执行 #f8e2d1a3 时遇到权限问题，请接手此任务，
               从已完成的 40% 继续",
      task_id: "f8e2d1a3"
    )
  
  决策 2 - 调整范围：
    回复 Agent C：
    send_notification(
      to: "agent-c",
      message: "@agent-c 请跳过私有仓库部分，专注于公共依赖分析",
      task_id: "f8e2d1a3"
    )
  
  决策 3 - 部分完成：
    task_complete(
      task_id: "f8e2d1a3",
      result: {
        "status": "partial",
        "completed_parts": ["公共依赖分析"],
        "blocked_parts": ["私有仓库访问"],
        "reason": "权限限制"
      }
    )
```

### 6.4.3 Agent 无响应

```
超过 20 分钟无响应：

Agent A 自动处理：
  - 通知统筹者（如果存在上级）
  - 在任务历史记录中标记异常
  
  可选：自动重新分配
  task_list(status: "active", assignee: "agent-b")
  找到其他可用的 Agent
  
  send_notification(
    to: "agent-b",
    message: "@agent-b 请接手任务 #f8e2d1a3，原执行者 agent-c 失联",
    task_id: "f8e2d1a3"
  )
```

## 6.5 心跳日志

每次心跳检查记录日志：

```
[2024-01-15T09:00:00Z] 心跳检查: 2 个活跃任务
[2024-01-15T09:00:00Z] 任务 #a7b3c9d2: 正常 (3 分钟前更新)
[2024-01-15T09:00:00Z] 任务 #f8e2d1a3: 正常 (5 分钟前更新)

[2024-01-15T09:15:00Z] 心跳检查: 2 个活跃任务
[2024-01-15T09:15:00Z] 任务 #a7b3c9d2: 正常 (18 分钟前更新)
[2024-01-15T09:15:00Z] ⚠️ 任务 #f8e2d1a3: 停滞 (15 分钟前更新)
[2024-01-15T09:15:00Z] 已向 agent-c 发送状态询问

[2024-01-15T09:20:00Z] 收到 agent-c 关于 #f8e2d1a3 的回复
[2024-01-15T09:20:00Z] 任务 #f8e2d1a3: 恢复监控
```

## 6.6 配置调整

在 CONFIG.toml 中可调整心跳参数：

```toml
[heartbeat]
enabled = true
interval_seconds = 30
stale_threshold_minutes = 10
query_timeout_minutes = 20

# 通知设置
notify_on_stale = true
notify_on_unresponsive = true
auto_reassign = false  # 是否自动重新分配无响应任务
```

## 6.7 设计要点

1. **被动监控**: 通过检查任务文件更新时间，不打扰 Agent 正常执行
2. **渐进式干预**: 先询问 → 等待回复 → 超时处理
3. **配置灵活**: 阈值可根据任务类型调整
4. **状态持久**: pending_queries 在内存中，重启后重新检测
5. **人机结合**: 自动检测 + LLM 决策后续处理
