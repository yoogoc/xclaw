use std::sync::Arc;
use rig::completion::CompletionModel;

pub struct LlmProvider<M: CompletionModel> {
    llm: Arc<M>
}