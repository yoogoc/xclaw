mod heartbeat;

use crate::config::AgentConfig;
use crate::llm::LlmClient;
use crate::tools::{ToolCall, ToolExecutor};
use anyhow::Result;
use chrono::{DateTime, Utc};
use log::info;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentState {
    pub current_task: Option<String>,
    pub conversation_history: Vec<Message>,
    pub last_update: DateTime<Utc>,
}

pub struct Agent {
    pub id: String,
    pub config: AgentConfig,
    pub llm_client: Arc<dyn LlmClient>,
    pub tool_executor: Arc<ToolExecutor>,
    pub state: Arc<RwLock<AgentState>>,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        llm_client: Arc<dyn LlmClient>,
        tool_executor: Arc<ToolExecutor>,
    ) -> Self {
        let state = AgentState {
            current_task: None,
            conversation_history: Vec::new(),
            last_update: Utc::now(),
        };

        Agent {
            id: config.name.clone(),
            config,
            llm_client,
            tool_executor,
            state: Arc::new(RwLock::new(state)),
        }
    }

    pub async fn process_message(&self, message: Message) -> Result<Vec<Message>> {
        let mut responses = Vec::new();

        // Update conversation history
        {
            let mut state = self.state.write().await;
            state.conversation_history.push(message.clone());
            state.last_update = Utc::now();

            // Extract task_id
            if let Some(task_id) = extract_task_id(&message.content) {
                state.current_task = Some(task_id);
            }
        }

        // Build prompt
        let prompt = self.build_prompt(&message).await?;

        // Call LLM
        info!("Agent {} calling LLM with prompt", self.id);
        let response = self.llm_client.complete(&prompt).await?;

        // Parse tool calls
        let tool_calls = parse_tool_calls(&response);
        if !tool_calls.is_empty() {
            info!("Agent {} executing {} tool calls", self.id, tool_calls.len());

            for call in tool_calls {
                let result = self.tool_executor.execute(call).await?;
                responses.push(Message {
                    role: "assistant".to_string(),
                    content: result,
                    timestamp: Utc::now(),
                    task_id: self.state.read().await.current_task.clone(),
                });
            }
        } else {
            // Return LLM response directly
            responses.push(Message {
                role: "assistant".to_string(),
                content: response,
                timestamp: Utc::now(),
                task_id: self.state.read().await.current_task.clone(),
            });
        }

        Ok(responses)
    }

    async fn build_prompt(&self, message: &Message) -> Result<String> {
        let state = self.state.read().await;

        let mut prompt = String::new();

        // System prompt
        prompt.push_str("You are an AI Agent responsible for processing tasks.\n\n");

        // Role definition
        prompt.push_str(&format!("## Your Identity\nAgent ID: {}\n\n", self.id));

        // Tool instructions
        if !self.config.tools.is_empty() {
            prompt.push_str("## Available Tools\n");
            for tool in &self.config.tools {
                prompt.push_str(&format!("- {}\n", tool));
            }
            prompt.push_str("\n");
        }

        // Important rules
        prompt.push_str("## Important Rules\n");
        prompt.push_str("1. All task-related tool calls must include task_id\n");
        prompt.push_str("2. Use the following format to call tools:\n");
        prompt.push_str("   <tool_call>{ \"name\": \"tool_name\", \"params\": {...} }</tool_call>\n");
        prompt.push_str("3. Use #shortID format to reference tasks in messages\n\n");

        // Current task
        if let Some(task_id) = &state.current_task {
            prompt.push_str(&format!("## Current Task\nTask ID: {}\n\n", task_id));
        }

        // Conversation history
        if state.conversation_history.len() > 1 {
            prompt.push_str("## Conversation History\n");
            for msg in state.conversation_history.iter().rev().take(5) {
                prompt.push_str(&format!("{}: {}\n", msg.role, msg.content));
            }
            prompt.push_str("\n");
        }

        // Current message
        prompt.push_str(&format!("## Current Message\n{}: {}\n", message.role, message.content));

        Ok(prompt)
    }
}

pub struct AgentLoop {
    agent: Arc<Agent>,
    message_receiver: mpsc::Receiver<Message>,
    response_sender: mpsc::Sender<Vec<Message>>,
}

impl AgentLoop {
    pub fn new(
        agent: Arc<Agent>,
        message_receiver: mpsc::Receiver<Message>,
        response_sender: mpsc::Sender<Vec<Message>>,
    ) -> Self {
        AgentLoop {
            agent,
            message_receiver,
            response_sender,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Agent {} starting loop", self.agent.id);

        while let Some(message) = self.message_receiver.recv().await {
            info!("Agent {} received message: {}", self.agent.id, message.content);

            match self.agent.process_message(message).await {
                Ok(responses) => {
                    for response in &responses {
                        info!("Agent {} response: {}", self.agent.id, response.content);
                    }
                    let _ = self.response_sender.send(responses).await;
                }
                Err(e) => {
                    log::error!("Agent {} error: {}", self.agent.id, e);
                }
            }
        }

        info!("Agent {} loop ended", self.agent.id);
        Ok(())
    }
}

fn extract_task_id(content: &str) -> Option<String> {
    let re = Regex::new(r"#([a-f0-9]{8})").ok()?;
    re.captures(content).map(|cap| cap[1].to_string())
}

fn parse_tool_calls(response: &str) -> Vec<ToolCall> {
    let re = Regex::new(r"<tool_call>(.*?)</tool_call>").unwrap();
    let mut calls = Vec::new();

    for cap in re.captures_iter(response) {
        if let Ok(call) = serde_json::from_str::<ToolCall>(&cap[1]) {
            calls.push(call);
        }
    }

    calls
}