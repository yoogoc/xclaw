use std::sync::Arc;
use rig::completion::CompletionModel;
use crate::agent::config::AgentLoopConfig;
use crate::agent::workspace::Workspace;
use crate::hooks::HookRegistry;
use crate::llm::LlmProvider;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;
use crate::storage::Database;

mod heartbeat;
mod workspace;
mod config;

pub struct Agent<M: CompletionModel> {
    pub storage: Option<Arc<Database>>,

    pub llm: Arc<LlmProvider<M>>,
    pub fast_llm: Option<Arc<LlmProvider<M>>>,
    pub workspace: Arc<Workspace>,

    pub skills: Option<Arc<std::sync::RwLock<SkillRegistry>>>,
    pub hooks: Arc<HookRegistry>,
    pub tools: Arc<ToolRegistry>,

    pub heartbeat: Option<Arc<heartbeat::Heartbeat>>,

    pub config: AgentLoopConfig,
}
