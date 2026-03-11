use anyhow::Result;
use async_trait::async_trait;
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use crate::config::{Config, LlmConfig};

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
}

// Mock LLM client for testing
pub struct MockLlmClient;

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        info!("Mock LLM called with prompt length: {}", prompt.len());

        // Return mock responses based on prompt content
        if prompt.contains("认领任务") || prompt.contains("claim task") {
            if prompt.contains("#a7b3c9d2") {
                return Ok(r#"我将认领任务 #a7b3c9d2。

<tool_call>{ "name": "task_claim", "params": { "task_id": "a7b3c9d2", "agent_id": "executor" } }</tool_call>"#.to_string());
            }
        }

        Ok("收到消息，正在处理中...".to_string())
    }
}

// Anthropic client implementation
pub struct AnthropicClient {
    api_key: String,
    base_url: String,
    model: String,
}

impl AnthropicClient {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        AnthropicClient {
            api_key,
            base_url,
            model,
        }
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        info!("Calling Anthropic API with model: {}", self.model);

        // TODO: Implement actual Anthropic API call using rig-core
        // For now, use mock implementation
        MockLlmClient.complete(prompt).await
    }
}

// OpenAI client implementation
pub struct OpenAiClient {
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiClient {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        OpenAiClient {
            api_key,
            base_url,
            model,
        }
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        info!("Calling OpenAI API with model: {}", self.model);

        // TODO: Implement actual OpenAI API call using rig-core
        // For now, use mock implementation
        MockLlmClient.complete(prompt).await
    }
}

pub fn create_llm_clients(config: &Config) -> Result<HashMap<String, Arc<dyn LlmClient>>> {
    let mut clients = HashMap::new();

    for llm_config in &config.llms {
        let client: Arc<dyn LlmClient> = if let Some(anthropic) = &llm_config.anthropic {
            Arc::new(AnthropicClient::new(
                anthropic.token.clone(),
                anthropic.base_url.clone(),
                llm_config.model.clone(),
            ))
        } else if let Some(openai) = &llm_config.openai {
            Arc::new(OpenAiClient::new(
                openai.token.clone(),
                openai.base_url.clone(),
                llm_config.model.clone(),
            ))
        } else {
            // Default to Mock client
            Arc::new(MockLlmClient)
        };

        clients.insert(llm_config.name.clone(), client);
    }

    Ok(clients)
}