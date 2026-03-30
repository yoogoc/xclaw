mod response;

use rig::completion::CompletionModel;
use std::sync::Arc;

pub use response::*;

pub struct LlmProvider<M: CompletionModel> {
    pub llm: Arc<M>,
}
