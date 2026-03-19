use crate::llm::LLMResponse;
use crate::session::PendingApproval;

/// Final outcome of the agentic loop.
pub enum LoopOutcome {
    /// Completed with a text response.
    Response(Box<LLMResponse>),
    /// Loop was stopped by a signal.
    Stopped,
    /// Max iterations exceeded.
    MaxIterations,
    /// A tool requires user approval before continuing (chat delegate only).
    ToolCall {
        approvals: Vec<Box<PendingApproval>>,
        not_found: Vec<String>,
    },
}
