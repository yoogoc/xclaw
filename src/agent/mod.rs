use crate::agent::config::AgentLoopConfig;
use crate::agent::workspace::Workspace;
use crate::hooks::HookRegistry;
use crate::llm::LlmProvider;
use crate::skills::SkillManager;
use crate::storage::Database;
use rig::completion::CompletionModel;
use std::sync::Arc;

pub mod config;
mod heartbeat;
pub mod workspace;

// 不能直接使用rig的agent，因为我们要自定义tool call的逻辑
pub struct Agent<M: CompletionModel> {
    pub storage: Option<Arc<Database>>,

    pub llm: Arc<LlmProvider<M>>,
    pub fast_llm: Option<Arc<LlmProvider<M>>>,
    pub workspace: Arc<Workspace>,

    pub skills: Option<Arc<SkillManager>>,
    pub hooks: Arc<HookRegistry>,

    pub heartbeat: Option<Arc<heartbeat::Heartbeat>>,

    pub config: AgentLoopConfig,
}
