use crate::message::{ChatMessage, Role, ToolCall};
use crate::session::approval::PendingApproval;
use crate::session::turn::Turn;
use crate::utils::truncate_preview;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// State of a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadState {
    /// Thread is idle, waiting for input.
    Idle,
    /// Thread is processing a turn.
    Processing,
    /// Thread is waiting for user approval.
    AwaitingApproval,
    /// Thread has completed (no more turns expected).
    Completed,
    /// Thread was interrupted.
    Interrupted,
}

/// A conversation thread within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Unique thread ID.
    pub id: Uuid,
    /// Parent session ID.
    pub session_id: Uuid,
    /// External routing identity.
    pub user_id: String,
    pub channel: String,
    pub external_thread_id: Option<String>,
    /// Current state.
    pub state: ThreadState,
    /// Turns in this thread.
    pub turns: Vec<Turn>,
    /// When the thread was created.
    pub created_at: DateTime<Utc>,
    /// When the thread was last updated.
    pub updated_at: DateTime<Utc>,
    /// Thread metadata (e.g., title, tags).
    pub metadata: serde_json::Value,
    /// Pending approval requests (when state is AwaitingApproval).
    #[serde(default)]
    pub pending_approvals: Vec<PendingApproval>,
}

impl Thread {
    /// Create a new thread.
    pub fn new(session_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            session_id,
            user_id: String::new(),
            channel: String::new(),
            external_thread_id: None,
            state: ThreadState::Idle,
            turns: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Null,
            pending_approvals: vec![],
        }
    }

    /// Create a thread with routing identities.
    pub fn with_routing(session_id: Uuid, user_id: impl Into<String>, channel: impl Into<String>, external_thread_id: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            session_id,
            user_id: user_id.into(),
            channel: channel.into(),
            external_thread_id,
            state: ThreadState::Idle,
            turns: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: serde_json::Value::Null,
            pending_approvals: vec![],
        }
    }
}

impl Thread {
    /// Get the current turn number (1-indexed for display).
    pub fn turn_number(&self) -> usize {
        self.turns.len() + 1
    }

    /// Get the last turn.
    pub fn last_turn(&self) -> Option<&Turn> {
        self.turns.last()
    }

    /// Get the last turn mutably.
    pub fn last_turn_mut(&mut self) -> Option<&mut Turn> {
        self.turns.last_mut()
    }

    /// Start a new turn with user input.
    pub fn start_turn(&mut self, user_input: impl Into<String>) -> &mut Turn {
        let turn_number = self.turns.len();
        let turn = Turn::new(self.session_id, self.id, turn_number, user_input);
        self.turns.push(turn);
        self.state = ThreadState::Processing;
        self.updated_at = Utc::now();
        // turn_number was len() before push, so it's a valid index after push
        &mut self.turns[turn_number]
    }

    /// Complete the current turn with a response.
    pub fn complete_turn(&mut self, response: Option<String>) {
        if let Some(turn) = self.turns.last_mut() {
            turn.complete(response);
        }
        self.state = ThreadState::Idle;
        self.updated_at = Utc::now();
    }

    /// Fail the current turn with an error.
    pub fn fail_turn(&mut self, error: impl Into<String>) {
        if let Some(turn) = self.turns.last_mut() {
            turn.fail(error);
        }
        self.state = ThreadState::Idle;
        self.updated_at = Utc::now();
    }
}

impl Thread {
    // /// Mark the thread as awaiting approval with pending request details.
    // pub fn await_approval(&mut self, pending: PendingApproval) {
    //     self.state = ThreadState::AwaitingApproval;
    //     self.pending_approval = Some(pending);
    //     self.updated_at = Utc::now();
    // }
    //
    // /// Take the pending approval (clearing it from the thread).
    // pub fn take_pending_approval(&mut self) -> Option<PendingApproval> {
    //     self.pending_approval.take()
    // }
    //
    // /// Clear pending approval and return to idle state.
    // pub fn clear_pending_approval(&mut self) {
    //     self.pending_approval = None;
    //     self.state = ThreadState::Idle;
    //     self.updated_at = Utc::now();
    // }
    //
    // /// Enter auth mode: next user message will be routed directly to
    // /// the credential store, bypassing the normal pipeline entirely.
    // pub fn enter_auth_mode(&mut self, extension_name: String) {
    //     self.pending_auth = Some(PendingAuth { extension_name });
    //     self.updated_at = Utc::now();
    // }
    //
    // /// Take the pending auth (clearing auth mode).
    // pub fn take_pending_auth(&mut self) -> Option<PendingAuth> {
    //     self.pending_auth.take()
    // }
}

impl Thread {
    /// Interrupt the current turn.
    pub fn interrupt(&mut self) {
        if let Some(turn) = self.turns.last_mut() {
            turn.interrupt();
        }
        self.state = ThreadState::Interrupted;
        self.updated_at = Utc::now();
    }

    /// Resume after interruption.
    pub fn resume(&mut self) {
        if self.state == ThreadState::Interrupted {
            self.state = ThreadState::Idle;
            self.updated_at = Utc::now();
        }
    }

    /// Get all messages for context building, including tool call history.
    ///
    /// Emits the full LLM-compatible message sequence per turn:
    /// `user → [assistant_with_tool_calls → tool_result*] → assistant`
    ///
    /// This ensures the LLM sees prior tool executions and won't re-attempt
    /// completed actions in subsequent turns.
    pub fn messages(&self) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        for turn in &self.turns {
            if turn.attachments.is_empty() {
                messages.push(ChatMessage::user(&turn.user_input));
            } else {
                messages.push(ChatMessage::user_with_attachments(&turn.user_input, turn.attachments.clone()));
            }

            if !turn.tool_calls.is_empty() {
                // Build ToolCall objects with synthetic stable IDs
                let tool_calls: Vec<ToolCall> = turn
                    .tool_calls
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| ToolCall {
                        id: format!("turn{}_{}", turn.turn_number, i),
                        name: tc.name.clone(),
                        arguments: tc.parameters.clone(),
                    })
                    .collect();

                // Assistant message declaring the tool calls (no text content)
                messages.push(ChatMessage::assistant_with_tool_calls(None, tool_calls));

                // Individual tool result messages, truncated to limit context size.
                for (i, tc) in turn.tool_calls.iter().enumerate() {
                    let call_id = format!("turn{}_{}", turn.turn_number, i);
                    let content = if let Some(ref err) = tc.error {
                        // .error already contains the full error text;
                        // pass through without wrapping to avoid double-prefix.
                        truncate_preview(err, 1000)
                    } else if let Some(ref res) = tc.result {
                        let raw = match res {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        truncate_preview(&raw, 1000)
                    } else {
                        "OK".to_string()
                    };
                    messages.push(ChatMessage::tool_result(call_id, &tc.name, content));
                }
            }
            if let Some(ref response) = turn.response {
                messages.push(ChatMessage::assistant(response));
            }
        }
        messages
    }

    /// Truncate turns to a specific count (keeping most recent).
    pub fn truncate_turns(&mut self, keep: usize) {
        if self.turns.len() > keep {
            let drain_count = self.turns.len() - keep;
            self.turns.drain(0..drain_count);
            // Re-number remaining turns
            for (i, turn) in self.turns.iter_mut().enumerate() {
                turn.turn_number = i;
            }
        }
    }

    /// Restore thread state from a checkpoint's messages.
    ///
    /// Clears existing turns and rebuilds from the message sequence.
    /// Handles the full message pattern including tool messages:
    /// `user → [assistant_with_tool_calls → tool_result*] → assistant`
    ///
    /// Also supports the legacy pattern (user/assistant pairs only) for
    /// backward compatibility with old checkpoint data.
    pub fn restore_from_messages(&mut self, messages: Vec<ChatMessage>) {
        self.turns.clear();
        self.state = ThreadState::Idle;

        let mut iter = messages.into_iter().peekable();
        let mut turn_number = 0;

        while let Some(msg) = iter.next() {
            if msg.role == Role::User {
                let mut turn = Turn::new(self.session_id, self.id, turn_number, &msg.content);

                // Consume tool call sequences (assistant_with_tool_calls + tool_results).
                // A single turn may contain multiple rounds of tool calls, so we
                // track the cumulative base index into turn.tool_calls.
                while let Some(next) = iter.peek() {
                    if next.role == Role::Assistant && next.tool_calls.is_some() {
                        let call_base_idx = turn.tool_calls.len();

                        if let Some(assistant_msg) = iter.next()
                            && let Some(ref tcs) = assistant_msg.tool_calls
                        {
                            for tc in tcs {
                                turn.record_tool_call(&tc.name, tc.arguments.clone());
                            }
                        }

                        // Consume the corresponding tool_result messages,
                        // indexing relative to this batch's base offset.
                        let mut pos = 0;
                        while let Some(tr) = iter.peek() {
                            if tr.role != Role::Tool {
                                break;
                            }
                            if let Some(tool_msg) = iter.next() {
                                let idx = call_base_idx + pos;
                                if idx < turn.tool_calls.len() {
                                    // Store as result — the error/success distinction
                                    // is for the live turn only; restored context just
                                    // needs the content the LLM originally saw.
                                    turn.tool_calls[idx].result = Some(serde_json::Value::String(tool_msg.content.clone()));
                                }
                            }
                            pos += 1;
                        }
                    } else {
                        break;
                    }
                }

                // Check if next is the final assistant response for this turn
                let is_final_assistant = iter.peek().is_some_and(|n| n.role == Role::Assistant && n.tool_calls.is_none());
                if is_final_assistant && let Some(response) = iter.next() {
                    turn.complete(Some(response.content));
                }

                self.turns.push(turn);
                turn_number += 1;
            } else {
                // Skip non-user messages that aren't anchored to a turn
                continue;
            }
        }

        self.updated_at = Utc::now();
    }
}
