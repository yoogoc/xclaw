use crate::session::PendingApproval;

/// Final outcome of the agentic loop.
pub enum LoopOutcome {
    /// Completed with a text response.
    Response(String),
    /// Loop was stopped by a signal.
    Stopped,
    /// Max iterations exceeded.
    MaxIterations,
    /// A tool requires user approval before continuing (chat delegate only).
    NeedApproval(Box<PendingApproval>),
    AutoApproval(Box<PendingApproval>),
}