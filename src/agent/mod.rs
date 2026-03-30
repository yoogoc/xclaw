use crate::agent::config::AgentLoopConfig;
use crate::agent::workspace::Workspace;
use crate::hooks::HookRegistry;
use crate::llm::LlmProvider;
use crate::skills::SkillRegistry;
use crate::storage::Database;
use crate::tools::{Tool, ToolRegistry};
use rig::completion::CompletionModel;
use std::collections::HashMap;
use std::sync::Arc;

mod config;
mod heartbeat;
mod workspace;

// 不能直接使用rig的agent，因为我们要自定义tool call的逻辑
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

impl<M: CompletionModel> Agent<M> {
    pub async fn tools(&self) -> HashMap<String, Arc<dyn Tool>> {
        self.tools.tools().await.clone()
    }
}
