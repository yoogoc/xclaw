use anyhow::Result;
use log::info;
use serenity::all::ChannelId;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt};
use xclaw::agent::Agent;
use xclaw::binding::Binding;
use xclaw::channel::{ChannelManager, DiscordChannel, DiscordConfig, WebSocketChannel};
use xclaw::config::Config;
use xclaw::hooks::HookRegistry;
use xclaw::llm::LlmProvider;
use xclaw::session::SessionManager;
use xclaw::storage::Database;
use xclaw::tools::ToolRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_span_events(fmt::format::FmtSpan::NONE)
        // .with_env_filter(EnvFilter::builder().parse("debug,tracing::span=off")?)
        .with_env_filter(EnvFilter::builder().parse("xclaw=debug,serenity=warn")?)
        .init();
    dotenv::dotenv().ok();

    info!("Starting xclaw");

    // Load config
    let config = Config::load("config.yaml")?;
    info!("Loaded config with {} bindings", config.bindings.len());

    // Create database and session manager
    let database_url = config.database_url();
    info!("Initializing database: {}", database_url);
    let db = Arc::new(Database::new(&database_url)?);
    let session_manager = Arc::new(SessionManager::new_with_db(db).await?);

    // Create bindings
    let mut tasks = vec![];

    for binding_config in &config.bindings {
        let binding_id = binding_config.get_binding_id();
        info!("Creating binding: {}", binding_id);

        // Find configs
        let agent_config = config.agents.iter().find(|a| a.name == binding_config.agent).ok_or_else(|| anyhow::anyhow!("Agent not found"))?;

        let llm_config = config.llms.iter().find(|l| l.name == agent_config.llm).ok_or_else(|| anyhow::anyhow!("LLM not found"))?;

        // Create components
        let llm = create_llm_provider(llm_config)?;
        let agent = create_agent(agent_config, llm)?;

        // Find channel config and create appropriate channel
        let channel_config = config
            .channels
            .iter()
            .find(|c| c.name == binding_config.channel)
            .ok_or_else(|| anyhow::anyhow!("Channel '{}' not found", binding_config.channel))?;
        let channel_manager = Arc::new(create_channel(channel_config).await?);

        // Create and spawn binding
        let binding = Binding::new(agent, channel_manager, session_manager.clone(), binding_id.clone(), chrono_tz::UTC);

        let task = tokio::spawn(async move {
            if let Err(e) = binding.start().await {
                log::error!("Binding error: {}", e);
            }
        });

        tasks.push(task);
    }

    info!("xclaw started successfully");

    // Keep running
    tokio::signal::ctrl_c().await?;
    info!("Shutting down");

    Ok(())
}

async fn create_channel(channel_config: &xclaw::config::ChannelConfig) -> Result<ChannelManager> {
    match channel_config.channel_type.as_str() {
        "discord" => {
            let channel = DiscordChannel::new(DiscordConfig {
                token: channel_config.token.clone(),
                channel_id: ChannelId::new(1478672180664598674),
                require_mention: false,
            })
            .await?;
            Ok(ChannelManager::new(channel))
        }
        _ => {
            let channel = WebSocketChannel::new();
            Ok(ChannelManager::new(channel))
        }
    }
}

fn create_llm_provider(llm_config: &xclaw::config::LlmConfig) -> Result<Arc<LlmProvider<rig::providers::anthropic::completion::CompletionModel>>> {
    use rig::client::CompletionClient;
    use rig::providers::anthropic::Client;

    let client = Client::builder()
        .base_url(&llm_config.anthropic.as_ref().unwrap().base_url)
        .api_key(&llm_config.anthropic.as_ref().unwrap().token)
        .build()?;
    let model = client.completion_model("claude-opus-4-6");

    Ok(Arc::new(LlmProvider { llm: Arc::new(model) }))
}

fn create_agent(
    _agent_config: &xclaw::config::AgentConfig,
    llm: Arc<LlmProvider<rig::providers::anthropic::completion::CompletionModel>>,
) -> Result<Arc<Agent<rig::providers::anthropic::completion::CompletionModel>>> {
    use std::path::PathBuf;

    let workspace = Arc::new(xclaw::agent::workspace::Workspace::new(PathBuf::from("workspace")));
    let hooks = Arc::new(HookRegistry::new());
    let tools = Arc::new(ToolRegistry::new());

    Ok(Arc::new(Agent {
        storage: None,
        llm,
        fast_llm: None,
        workspace,
        skills: None,
        hooks,
        tools,
        heartbeat: None,
        config: xclaw::agent::config::AgentLoopConfig {
            max_iterations: 10,
            enable_tool_intent_nudge: false,
            max_tool_intent_nudges: 3,
        },
    }))
}
