mod config;
mod agent;
mod llm;
mod channel;
mod tools;

use agent::{Agent, AgentLoop, Message};
use anyhow::Result;
use channel::MockChannel;
use chrono::Utc;
use config::Config;
use llm::create_llm_clients;
use log::info;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tools::{TaskClaimTool, TaskProgressTool, TaskReadTool, ToolExecutor, Task, TaskStatus};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();
    dotenv::dotenv().ok();

    info!("Starting xcraw minimal agent loop");

    // 1. Load configuration
    let config = Config::load("config.yaml")?;
    info!("Loaded config with {} agents", config.agents.len());

    // 2. Create workspace directory
    let workspace_path = PathBuf::from("workspace");
    tokio::fs::create_dir_all(workspace_path.join("tasks/pending")).await?;
    tokio::fs::create_dir_all(workspace_path.join("tasks/active")).await?;
    tokio::fs::create_dir_all(workspace_path.join("tasks/completed")).await?;
    tokio::fs::create_dir_all(workspace_path.join("tasks/failed")).await?;

    // 3. Create test task
    create_test_task(&workspace_path).await?;

    // 4. Create LLM clients
    let llm_clients = create_llm_clients(&config)?;

    // 5. Create tool executor
    let mut tool_executor = ToolExecutor::new();
    tool_executor.register(Arc::new(TaskReadTool::new(workspace_path.clone())));
    tool_executor.register(Arc::new(TaskClaimTool::new(workspace_path.clone())));
    tool_executor.register(Arc::new(TaskProgressTool::new(workspace_path.clone())));
    let tool_executor = Arc::new(tool_executor);

    // 6. Create Agents and start loops
    let mut handles = Vec::new();
    let mut channels = HashMap::new();

    for agent_config in config.agents {
        let agent_name = agent_config.name.clone();
        info!("Creating agent: {}", agent_name);

        // Get LLM client
        let llm_client = llm_clients
            .get(&agent_config.llm)
            .cloned()
            .unwrap_or_else(|| Arc::new(llm::MockLlmClient));

        // Create Agent
        let agent = Arc::new(Agent::new(
            agent_config,
            llm_client,
            tool_executor.clone(),
        ));

        // Create message channels
        let (msg_tx, msg_rx) = mpsc::channel(100);
        let (resp_tx, mut resp_rx) = mpsc::channel(100);

        channels.insert(agent_name.clone(), msg_tx.clone());

        // Start AgentLoop
        let agent_loop = AgentLoop::new(agent.clone(), msg_rx, resp_tx.clone());
        let handle = tokio::spawn(async move {
            if let Err(e) = agent_loop.run().await {
                log::error!("Agent loop error: {}", e);
            }
        });
        handles.push(handle);

        // Start response handler
        let agent_name_clone = agent_name.clone();
        tokio::spawn(async move {
            while let Some(responses) = resp_rx.recv().await {
                for response in responses {
                    info!("[{}] Response: {}", agent_name_clone, response.content);
                }
            }
        });
    }

    // 7. Simulate sending test message
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    if let Some(tx) = channels.get("executor") {
        info!("Sending test message to executor agent");
        let test_msg = Message {
            role: "user".to_string(),
            content: "Please claim task #a7b3c9d2 and start processing".to_string(),
            timestamp: Utc::now(),
            task_id: None,
        };
        tx.send(test_msg).await?;
    }

    // 8. Run for a while then shutdown
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    info!("Shutting down...");

    // Note: In production, implement graceful shutdown
    // for handle in handles {
    //     handle.abort();
    // }

    Ok(())
}

async fn create_test_task(workspace_path: &PathBuf) -> Result<()> {
    let task_id = "a7b3c9d2";
    let full_id = format!("{}-1234-5678-9abc-def012345678", task_id);

    let task = Task {
        task_id: full_id.clone(),
        short_id: task_id.to_string(),
        title: "Test Task".to_string(),
        description: "This is a test task to verify Agent loop functionality".to_string(),
        status: TaskStatus::Pending,
        assigned_to: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        progress: 0.0,
    };

    let path = workspace_path
        .join("tasks")
        .join("pending")
        .join(format!("{}.json", task_id));

    tokio::fs::write(path, serde_json::to_string_pretty(&task)?).await?;
    info!("Created test task: #{}", task_id);

    Ok(())
}
