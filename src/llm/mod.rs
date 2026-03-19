mod response;

use std::sync::Arc;
use rig::completion::CompletionModel;

pub use response::*;

pub struct LlmProvider<M: CompletionModel> {
    pub llm: Arc<M>
}