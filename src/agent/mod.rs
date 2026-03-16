use std::sync::Arc;
use rig::completion::CompletionModel;
use crate::agent::workspace::Workspace;
use crate::hooks::HookRegistry;
use crate::llm_provider::LlmProvider;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;
use crate::storage::Database;

mod heartbeat;
mod workspace;

pub struct Agent<M: CompletionModel> {
    pub storage: Option<Arc<Database>>,

    pub llm: Arc<LlmProvider<M>>,
    pub cheap_llm: Option<Arc<LlmProvider<M>>>,
    pub workspace: Option<Arc<Workspace>>,

    pub skills: Option<Arc<std::sync::RwLock<SkillRegistry>>>,
    pub hooks: Arc<HookRegistry>,
    pub tools: Arc<ToolRegistry>,

    pub heartbeat: Option<Arc<heartbeat::Heartbeat>>,
}