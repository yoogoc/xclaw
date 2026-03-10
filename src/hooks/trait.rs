//! Hook trait definitions

use crate::hooks::context::HookContext;
use crate::hooks::types::{
    HookResult, MessageHookResult, RoomMessage, Task, TaskError, ToolError, ToolHookResult,
};
use std::future::Future;
use std::pin::Pin;

/// Type alias for hook futures (needed for trait object compatibility)
pub type HookFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Base trait for all hooks
///
/// All hook implementations must implement this trait to provide
/// basic metadata like name, priority, and enabled status.
pub trait Hook: Send + Sync {
    /// Hook name (used for logging and configuration)
    fn name(&self) -> &str;

    /// Hook priority - lower numbers execute first (default: 100)
    fn priority(&self) -> i32 {
        100
    }

    /// Whether this hook is enabled (default: true)
    fn enabled(&self) -> bool {
        true
    }
}

/// Trait for hooks that respond to task lifecycle events
///
/// Implement this trait to hook into task creation, assignment,
/// progress updates, completion, and failure events.
pub trait TaskHook: Hook {
    /// Called when a new task is created
    fn on_task_created(
        &self,
        ctx: &HookContext,
        task: &Task,
    ) -> HookFuture<HookResult>;

    /// Called when a task is claimed by an agent
    fn on_task_claimed(
        &self,
        ctx: &HookContext,
        task: &Task,
    ) -> HookFuture<HookResult>;

    /// Called when task progress is updated
    fn on_task_progress(
        &self,
        ctx: &HookContext,
        task: &Task,
    ) -> HookFuture<HookResult>;

    /// Called when a task is completed successfully
    fn on_task_completed(
        &self,
        ctx: &HookContext,
        task: &Task,
    ) -> HookFuture<HookResult>;

    /// Called when a task fails
    fn on_task_failed(
        &self,
        ctx: &HookContext,
        task: &Task,
        error: &TaskError,
    ) -> HookFuture<HookResult>;

    /// Called when a task is cancelled
    fn on_task_cancelled(
        &self,
        ctx: &HookContext,
        task: &Task,
    ) -> HookFuture<HookResult>;

    /// Called when a task is detected as stalled (no progress for timeout period)
    fn on_task_stalled(
        &self,
        ctx: &HookContext,
        task: &Task,
    ) -> HookFuture<HookResult>;
}

/// Trait for hooks that respond to agent lifecycle events
///
/// Implement this trait to hook into agent joining/leaving
/// and responsiveness monitoring events.
pub trait AgentHook: Hook {
    /// Called when an agent joins the chat room
    fn on_agent_joined(
        &self,
        ctx: &HookContext,
        agent_id: &str,
    ) -> HookFuture<HookResult>;

    /// Called when an agent leaves the chat room
    fn on_agent_left(
        &self,
        ctx: &HookContext,
        agent_id: &str,
    ) -> HookFuture<HookResult>;

    /// Called on agent heartbeat update
    fn on_agent_heartbeat(
        &self,
        ctx: &HookContext,
        agent_id: &str,
    ) -> HookFuture<HookResult>;

    /// Called when an agent is detected as unresponsive
    fn on_agent_unresponsive(
        &self,
        ctx: &HookContext,
        agent_id: &str,
    ) -> HookFuture<HookResult>;
}

/// Trait for hooks that respond to message events
///
/// Implement this trait to intercept, filter, or respond to
/// messages sent and received in chat rooms.
pub trait MessageHook: Hook {
    /// Called when a message is received
    /// Can modify, block, or pass the message
    fn on_message_received(
        &self,
        ctx: &HookContext,
        msg: &RoomMessage,
    ) -> HookFuture<MessageHookResult>;

    /// Called when a message is sent
    fn on_message_sent(
        &self,
        ctx: &HookContext,
        msg: &RoomMessage,
    ) -> HookFuture<HookResult>;

    /// Called when an @mention is received
    fn on_mention_received(
        &self,
        ctx: &HookContext,
        msg: &RoomMessage,
    ) -> HookFuture<HookResult>;
}

/// Trait for hooks that respond to tool call events
///
/// Implement this trait to intercept, modify, or log
/// tool calls made by agents.
pub trait ToolHook: Hook {
    /// Called before a tool is executed
    /// Can modify parameters, block execution, or allow it to continue
    fn before_tool_call(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> HookFuture<ToolHookResult>;

    /// Called after a tool is executed successfully
    fn after_tool_call(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        result: &serde_json::Value,
    ) -> HookFuture<HookResult>;

    /// Called when a tool call fails
    fn on_tool_error(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        error: &ToolError,
    ) -> HookFuture<HookResult>;
}
