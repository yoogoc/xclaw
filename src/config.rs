use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub llms: Vec<LlmConfig>,
    pub agents: Vec<AgentConfig>,
    pub channels: Vec<ChannelConfig>,
    pub chat_rooms: Vec<ChatRoomConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anthropic: Option<AnthropicConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<OpenAiConfig>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub token: String,
    #[serde(default = "default_anthropic_url")]
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    pub token: String,
    #[serde(default = "default_openai_url")]
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub llm: String,
    #[serde(default = "default_true")]
    pub default_tools: bool,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default = "default_true")]
    pub default_skills: bool,
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRoomConfig {
    #[serde(rename = "type")]
    pub room_type: String,
    pub bindings: Vec<BindingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingConfig {
    pub agent: String,
    pub channel: String,
    #[serde(rename = "requireMention", default = "default_false")]
    pub require_mention: bool,
}

fn default_true() -> bool { true }
fn default_false() -> bool { false }
fn default_anthropic_url() -> String { "https://api.anthropic.com".into() }
fn default_openai_url() -> String { "https://api.openai.com".into() }

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let mut config: Config = serde_yaml::from_str(&content)?;

        // Process environment variables
        for llm in &mut config.llms {
            if let Some(anthropic) = &mut llm.anthropic {
                if anthropic.token.starts_with("${") && anthropic.token.ends_with("}") {
                    let var_name = &anthropic.token[2..anthropic.token.len()-1];
                    anthropic.token = std::env::var(var_name)?;
                }
            }
            if let Some(openai) = &mut llm.openai {
                if openai.token.starts_with("${") && openai.token.ends_with("}") {
                    let var_name = &openai.token[2..openai.token.len()-1];
                    openai.token = std::env::var(var_name)?;
                }
            }
        }

        Ok(config)
    }
}