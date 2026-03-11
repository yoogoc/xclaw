use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub params: Value,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, params: Value) -> Result<String>;
}

pub struct ToolExecutor {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolExecutor {
    pub fn new() -> Self {
        ToolExecutor {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub async fn execute(&self, call: ToolCall) -> Result<String> {
        if let Some(tool) = self.tools.get(&call.name) {
            info!("Executing tool: {} with params: {}", call.name, call.params);
            tool.execute(call.params).await
        } else {
            Err(anyhow::anyhow!("Tool not found: {}", call.name))
        }
    }
}

// Task data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub short_id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub assigned_to: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Active,
    Completed,
    Failed,
}

// Read task tool
pub struct TaskReadTool {
    workspace_path: PathBuf,
}

impl TaskReadTool {
    pub fn new(workspace_path: PathBuf) -> Self {
        TaskReadTool { workspace_path }
    }
}

#[async_trait]
impl Tool for TaskReadTool {
    fn name(&self) -> &str {
        "task_read"
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing task_id parameter"))?;

        // Search for task file
        for status in ["pending", "active", "completed", "failed"] {
            let path = self.workspace_path
                .join("tasks")
                .join(status)
                .join(format!("{}.json", task_id));

            if path.exists() {
                let content = tokio::fs::read_to_string(&path).await?;
                let task: Task = serde_json::from_str(&content)?;
                return Ok(format!(
                    "Task #{} - {}\nStatus: {:?}\nProgress: {}%",
                    task.short_id, task.title, task.status, task.progress
                ));
            }
        }

        Err(anyhow::anyhow!("Task {} not found", task_id))
    }
}

// Claim task tool
pub struct TaskClaimTool {
    workspace_path: PathBuf,
}

impl TaskClaimTool {
    pub fn new(workspace_path: PathBuf) -> Self {
        TaskClaimTool { workspace_path }
    }
}

#[async_trait]
impl Tool for TaskClaimTool {
    fn name(&self) -> &str {
        "task_claim"
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing task_id"))?;

        let agent_id = params["agent_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;

        let pending_path = self.workspace_path
            .join("tasks")
            .join("pending")
            .join(format!("{}.json", task_id));

        let active_path = self.workspace_path
            .join("tasks")
            .join("active")
            .join(format!("{}.json", task_id));

        if !pending_path.exists() {
            return Err(anyhow::anyhow!("Task {} not found in pending", task_id));
        }

        // Read and update task
        let content = tokio::fs::read_to_string(&pending_path).await?;
        let mut task: Task = serde_json::from_str(&content)?;

        task.status = TaskStatus::Active;
        task.assigned_to = Some(agent_id.to_string());
        task.updated_at = Utc::now();

        // Save to active directory
        tokio::fs::create_dir_all(active_path.parent().unwrap()).await?;
        tokio::fs::write(&active_path, serde_json::to_string_pretty(&task)?).await?;
        tokio::fs::remove_file(&pending_path).await?;

        Ok(format!("Task #{} has been claimed by {}", task.short_id, agent_id))
    }
}

// Update progress tool
pub struct TaskProgressTool {
    workspace_path: PathBuf,
}

impl TaskProgressTool {
    pub fn new(workspace_path: PathBuf) -> Self {
        TaskProgressTool { workspace_path }
    }
}

#[async_trait]
impl Tool for TaskProgressTool {
    fn name(&self) -> &str {
        "task_progress"
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing task_id"))?;

        let progress = params["progress"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("Missing progress"))? as f32;

        let active_path = self.workspace_path
            .join("tasks")
            .join("active")
            .join(format!("{}.json", task_id));

        if !active_path.exists() {
            return Err(anyhow::anyhow!("Task {} not found in active", task_id));
        }

        // Update task progress
        let content = tokio::fs::read_to_string(&active_path).await?;
        let mut task: Task = serde_json::from_str(&content)?;

        task.progress = progress;
        task.updated_at = Utc::now();

        // If completed, move to completed
        if progress >= 100.0 {
            task.status = TaskStatus::Completed;

            let completed_path = self.workspace_path
                .join("tasks")
                .join("completed")
                .join(format!("{}.json", task_id));

            tokio::fs::create_dir_all(completed_path.parent().unwrap()).await?;
            tokio::fs::write(&completed_path, serde_json::to_string_pretty(&task)?).await?;
            tokio::fs::remove_file(&active_path).await?;

            Ok(format!("Task #{} completed ✅", task.short_id))
        } else {
            tokio::fs::write(&active_path, serde_json::to_string_pretty(&task)?).await?;
            Ok(format!("Task #{} progress updated: {}%", task.short_id, progress))
        }
    }
}