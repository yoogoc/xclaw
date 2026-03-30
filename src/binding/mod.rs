mod loop_outcome;
mod loop_type;
mod intent;

use crate::agent::Agent;
use crate::binding::loop_outcome::LoopOutcome;
use crate::binding::loop_type::LoopType;
use crate::binding::intent::Intent;
use crate::channel::{Channel, ChannelManager, IncomingMessage};
use crate::llm::{FinishReason, LLMResponse};
use crate::session::{PendingApproval, Session, SessionManager, ThreadState};
use crate::tools::ApprovalRequirement;
use futures::StreamExt;
use rig::OneOrMany;
use rig::completion::{CompletionModel, CompletionRequest};
use rig::streaming::StreamedAssistantContent;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct Binding<M: CompletionModel, C: Channel> {
    agent: Arc<Agent<M>>,
    channel: Arc<ChannelManager<C>>,
    session_manager: Arc<SessionManager>,
    binding_id: String,
    user_tz: chrono_tz::Tz,
}

impl<M: CompletionModel, C: Channel> Binding<M, C> {
    pub fn new(
        agent: Arc<Agent<M>>,
        channel: Arc<ChannelManager<C>>,
        session_manager: Arc<SessionManager>,
        binding_id: impl Into<String>,
        tz: chrono_tz::Tz,
    ) -> Self {
        Self {
            agent,
            channel,
            session_manager,
            binding_id: binding_id.into(),
            user_tz: tz,
        }
    }
}

impl<M: CompletionModel, C: Channel> Binding<M, C> {
    pub async fn start(&self) -> anyhow::Result<()> {
        let mut stream = self.channel.receive().await?;

        while let Some(message) = stream.next().await {
            // get session, thread_id
            self.handle_message(&message).await?;
        }
        Ok(())
    }

    pub async fn handle_message(&self, message: &IncomingMessage) -> anyhow::Result<()> {
        // Step 1: Parse intent
        let intent = Intent::parse(message);

        // Step 2: Resolve session & thread
        let (session, thread_id) = self
            .session_manager
            .resolve_thread(
                &self.binding_id,
                &message.user_id,
                &message.channel,
                message.thread_id.as_deref(),
            )
            .await;

        // Step 3: Get thread state
        let thread_state = {
            let sess = session.lock().await;
            sess.threads.get(&thread_id)
                .map(|t| t.state)
                .unwrap_or(ThreadState::Idle)
        };

        // Step 4: Dispatch by intent and state
        match (intent, thread_state) {
            (Intent::UserInput, ThreadState::Idle) => {
                self.process_user_input(session, thread_id, message).await
            }
            (Intent::UserInput, ThreadState::AwaitingApproval) => {
                // Interrupt current turn and start new one
                {
                    let mut sess = session.lock().await;
                    if let Some(thread) = sess.threads.get_mut(&thread_id) {
                        thread.interrupt();
                    }
                }
                self.process_user_input(session, thread_id, message).await
            }
            (Intent::ApprovalAccept, ThreadState::AwaitingApproval) => {
                self.process_approval(session, thread_id, true, false).await
            }
            (Intent::ApprovalAlways, ThreadState::AwaitingApproval) => {
                self.process_approval(session, thread_id, true, true).await
            }
            (Intent::ApprovalReject, ThreadState::AwaitingApproval) => {
                self.process_approval(session, thread_id, false, false).await
            }
            (Intent::Interrupt, _) => {
                let mut sess = session.lock().await;
                if let Some(thread) = sess.threads.get_mut(&thread_id) {
                    thread.interrupt();
                }
                Ok(())
            }
            _ => Ok(()), // Ignore invalid combinations
        }
    }

    pub async fn process_user_input(
        &self,
        session: Arc<Mutex<Session>>,
        thread_id: Uuid,
        message: &IncomingMessage,
    ) -> anyhow::Result<()> {
        // Start new turn
        {
            let mut sess = session.lock().await;
            let thread = sess.threads.get_mut(&thread_id).unwrap();
            thread.start_turn(&message.content);
        }

        // Run agent loop
        self.run_loop(session, thread_id).await
    }

    pub async fn process_approval(
        &self,
        session: Arc<Mutex<Session>>,
        thread_id: Uuid,
        approved: bool,
        always: bool,
    ) -> anyhow::Result<()> {
        if approved {
            // Add to auto-approved if always
            if always {
                let tool_names: Vec<String> = {
                    let sess = session.lock().await;
                    sess.threads.get(&thread_id)
                        .map(|t| t.pending_approvals.iter().map(|a| a.tool_name.clone()).collect())
                        .unwrap_or_default()
                };

                let mut sess = session.lock().await;
                for tool_name in tool_names {
                    sess.auto_approve_tool(tool_name);
                }
            }

            // Continue loop
            self.run_loop(session, thread_id).await
        } else {
            // Reject: fail turn
            let mut sess = session.lock().await;
            if let Some(thread) = sess.threads.get_mut(&thread_id) {
                thread.fail_turn("Tool approval rejected");
            }
            Ok(())
        }
    }

    // agent core loop
    pub async fn run_loop(
        &self,
        session: Arc<Mutex<Session>>,
        thread_id: Uuid,
    ) -> anyhow::Result<()> {
        loop {
            let outcome = self.run_agentic_loop(session.clone(), thread_id).await?;

            match outcome {
                LoopOutcome::Response(_response) => {
                    // Complete turn
                    {
                        let mut sess = session.lock().await;
                        if let Some(thread) = sess.threads.get_mut(&thread_id) {
                            thread.complete_turn("Response completed");
                            thread.state = ThreadState::Idle;
                        }
                    }
                    break;
                }

                LoopOutcome::ToolCall { approvals, .. } => {
                    let all_auto = approvals.iter().all(|a| a.auto_approved);

                    if all_auto {
                        // Execute tools and continue
                        self.execute_tools(session.clone(), thread_id, &approvals).await?;
                        continue;
                    } else {
                        // Need approval
                        {
                            let mut sess = session.lock().await;
                            if let Some(thread) = sess.threads.get_mut(&thread_id) {
                                thread.state = ThreadState::AwaitingApproval;
                                thread.pending_approvals = approvals.into_iter().map(|b| *b).collect();
                            }
                        }
                        break;
                    }
                }

                LoopOutcome::MaxIterations => {
                    let mut sess = session.lock().await;
                    if let Some(thread) = sess.threads.get_mut(&thread_id) {
                        thread.fail_turn("Max iterations reached");
                    }
                    break;
                }

                LoopOutcome::Stopped => {
                    let mut sess = session.lock().await;
                    if let Some(thread) = sess.threads.get_mut(&thread_id) {
                        thread.interrupt();
                    }
                    break;
                }
            }
        }

        Ok(())
    }

    // agent core loop
    pub async fn run_agentic_loop(
        &self,
        session: Arc<Mutex<Session>>,
        thread_id: Uuid,
    ) -> anyhow::Result<LoopOutcome> {
        let current_iteration = {
            let sess = session.lock().await;
            sess.threads.get(&thread_id)
                .and_then(|t| t.last_turn())
                .map(|turn| turn.current_tool_iterations)
                .unwrap_or(0)
        };

        for iteration in current_iteration..self.agent.config.max_iterations {
            // Update iteration count
            {
                let mut sess = session.lock().await;
                if let Some(thread) = sess.threads.get_mut(&thread_id) {
                    if let Some(turn) = thread.last_turn_mut() {
                        turn.current_tool_iterations = iteration;
                    }
                }
            }

            // Call LLM (placeholder for now)
            let resp = self.call_llm(session.clone(), thread_id).await?;

            match resp.finish_reason {
                FinishReason::Stop => return Ok(LoopOutcome::Response(Box::new(resp))),
                FinishReason::ToolUse => {
                    let approvals = self.prepare_tool_approvals(session.clone(), &resp.tool_calls).await?;
                    return Ok(LoopOutcome::ToolCall { approvals, not_found: vec![] });
                }
                FinishReason::Length => continue,
                _ => return Err(anyhow::anyhow!("LLM error: {:?}", resp.finish_reason)),
            }
        }

        Ok(LoopOutcome::MaxIterations)
    }

    async fn prepare_tool_approvals(
        &self,
        session: Arc<Mutex<Session>>,
        tool_calls: &[crate::message::ToolCall],
    ) -> anyhow::Result<Vec<Box<PendingApproval>>> {
        let tools = self.agent.tools().await;
        let mut approvals = vec![];

        let is_auto_approved = {
            let sess = session.lock().await;
            sess.auto_approved_tools.clone()
        };

        for tc in tool_calls {
            if let Some(tool) = tools.get(&tc.name) {
                let auto_approved = match tool.requires_approval(&tc.arguments) {
                    ApprovalRequirement::Never => true,
                    ApprovalRequirement::UnlessAutoApproved => is_auto_approved.contains(&tc.name),
                    ApprovalRequirement::Always => false,
                };

                approvals.push(Box::new(PendingApproval {
                    auto_approved,
                    request_id: Uuid::new_v4(),
                    tool_name: tc.name.clone(),
                    parameters: tc.arguments.clone(),
                    display_parameters: serde_json::Value::Null,
                    description: tool.description().to_string(),
                    tool_call_id: tc.id.clone(),
                    context_messages: vec![],
                    user_timezone: Some(self.user_tz.name().to_string()),
                }));
            }
        }

        Ok(approvals)
    }

    async fn execute_tools(
        &self,
        session: Arc<Mutex<Session>>,
        thread_id: Uuid,
        approvals: &[Box<PendingApproval>],
    ) -> anyhow::Result<()> {
        let tools = self.agent.tools().await;

        for approval in approvals {
            if let Some(tool) = tools.get(&approval.tool_name) {
                // Record call
                {
                    let mut sess = session.lock().await;
                    if let Some(thread) = sess.threads.get_mut(&thread_id) {
                        if let Some(turn) = thread.last_turn_mut() {
                            turn.record_tool_call(&approval.tool_name, approval.parameters.clone());
                        }
                    }
                }

                // Execute
                let result = tool.execute(approval.parameters.clone()).await;

                // Record result
                {
                    let mut sess = session.lock().await;
                    if let Some(thread) = sess.threads.get_mut(&thread_id) {
                        if let Some(turn) = thread.last_turn_mut() {
                            match result {
                                Ok(output) => turn.record_tool_result(serde_json::to_value(output)?),
                                Err(e) => turn.record_tool_error(e.to_string()),
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn call_llm(
        &self,
        _session: Arc<Mutex<Session>>,
        _thread_id: Uuid,
    ) -> anyhow::Result<LLMResponse> {
        // TODO: Build context from thread
        let llm = self.agent.llm.llm.clone();
        let mut stream = llm.stream(CompletionRequest {
            model: None,
            preamble: None,
            chat_history: OneOrMany::many(vec![])?,
            documents: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        }).await?;

        while let Some(_content) = stream.next().await {}

        Ok(LLMResponse::from(stream.choice))
    }
}
