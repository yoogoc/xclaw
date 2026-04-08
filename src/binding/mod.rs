mod intent;
mod loop_outcome;
mod loop_type;
mod message_convert;

use crate::agent::Agent;
use crate::binding::intent::Intent;
use crate::binding::loop_outcome::LoopOutcome;
use crate::binding::message_convert::to_rig_messages;
use crate::channel::{ChannelManager, IncomingMessage};
use crate::llm::{FinishReason, LLMResponse};
use crate::session::{PendingApproval, Session, SessionManager, ThreadState};
use crate::tools::{ApprovalRequirement, ToolRegistry};
use futures::StreamExt;
use rig::OneOrMany;
use rig::completion::{CompletionModel, CompletionRequest, ToolDefinition};
use rig::message::ReasoningContent;
use rig::streaming::StreamedAssistantContent;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct Binding<M: CompletionModel> {
    agent: Arc<Agent<M>>,
    channel: Arc<ChannelManager>,
    session_manager: Arc<SessionManager>,
    tool_registry: Arc<ToolRegistry>,
    binding_id: String,
    user_tz: chrono_tz::Tz,
}

impl<M: CompletionModel> Binding<M> {
    pub fn new(
        agent: Arc<Agent<M>>,
        channel: Arc<ChannelManager>,
        session_manager: Arc<SessionManager>,
        binding_id: impl Into<String>,
        tz: chrono_tz::Tz,
    ) -> Self {
        Self {
            agent,
            channel,
            session_manager,
            tool_registry: Arc::new(ToolRegistry::new()),
            binding_id: binding_id.into(),
            user_tz: tz,
        }
    }
}

impl<M: CompletionModel> Binding<M> {
    pub async fn start(&self) -> anyhow::Result<()> {
        info!("start binding({})", &self.binding_id);

        // Start the channel (connect, authenticate, etc.)
        self.channel.start().await?;

        let mut stream = self.channel.receive().await?;

        while let Some(message) = stream.next().await {
            let result = {
                if let Some(id) = &message.external_id {
                    self.channel.send_read(&id).await?;
                }

                // get session, thread_id
                self.handle_message(&message).await
            };

            if let Err(err) = result {
                error!("[handle message]: {}", err);
            }
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
            sess.threads
                .get(&thread_id)
                .map(|t| t.state)
                .unwrap_or(ThreadState::Idle)
        };

        // Step 4: Dispatch by intent and state
        let result = match (intent.clone(), thread_state) {
            (Intent::UserInput, ThreadState::Idle) => {
                self.process_user_input(session.clone(), thread_id, message)
                    .await
            }
            (Intent::UserInput, ThreadState::AwaitingApproval) => {
                // Interrupt current turn and start new one
                {
                    let mut sess = session.lock().await;
                    if let Some(thread) = sess.threads.get_mut(&thread_id) {
                        thread.interrupt();
                    }
                }
                self.process_user_input(session.clone(), thread_id, message)
                    .await
            }
            (Intent::ApprovalAccept, ThreadState::AwaitingApproval) => {
                self.process_approval(session.clone(), thread_id, true, false)
                    .await
            }
            (Intent::ApprovalAlways, ThreadState::AwaitingApproval) => {
                self.process_approval(session.clone(), thread_id, true, true)
                    .await
            }
            (Intent::ApprovalReject, ThreadState::AwaitingApproval) => {
                self.process_approval(session.clone(), thread_id, false, false)
                    .await
            }
            (Intent::Interrupt, _) => {
                let mut sess = session.lock().await;
                if let Some(thread) = sess.threads.get_mut(&thread_id) {
                    thread.interrupt();
                }
                Ok(())
            }
            _ => {
                warn!(
                    "unhandled intent: {:?}, thread state: {:?}",
                    intent, thread_state
                );
                Ok(())
            } // Ignore invalid combinations
        };

        if let Err(err) = result {
            {
                let mut sess = session.lock().await;
                if let Some(thread) = sess.threads.get_mut(&thread_id) {
                    thread.fail_turn(err.to_string());
                }
            }

            let channel = self.channel.clone();
            let thread_id = message
                .thread_id
                .as_ref()
                .map_or(thread_id.to_string(), |t| t.to_string());
            channel.send(&thread_id, &err.to_string()).await?;
        }

        Ok(())
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
                    sess.threads
                        .get(&thread_id)
                        .map(|t| {
                            t.pending_approvals
                                .iter()
                                .map(|a| a.tool_name.clone())
                                .collect()
                        })
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
                        self.execute_tools(session.clone(), thread_id, &approvals)
                            .await?;
                        continue;
                    } else {
                        // Need approval
                        {
                            let mut sess = session.lock().await;
                            if let Some(thread) = sess.threads.get_mut(&thread_id) {
                                thread.state = ThreadState::AwaitingApproval;
                                thread.pending_approvals =
                                    approvals.into_iter().map(|b| *b).collect();
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
            sess.threads
                .get(&thread_id)
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
                    let approvals = self
                        .prepare_tool_approvals(session.clone(), &resp.tool_calls)
                        .await?;
                    return Ok(LoopOutcome::ToolCall {
                        approvals,
                        not_found: vec![],
                    });
                }
                FinishReason::Length => continue,
                FinishReason::Reasoning => continue,
                FinishReason::ContentFilter => {
                    return Err(anyhow::anyhow!("LLM error: {:?}", resp.finish_reason));
                }
                FinishReason::Unknown => {
                    return Err(anyhow::anyhow!("LLM error: {:?}", resp.finish_reason));
                }
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
                debug!("execute tool!");
                let result = tool.execute(approval.parameters.clone()).await;

                // Record result
                {
                    let mut sess = session.lock().await;
                    if let Some(thread) = sess.threads.get_mut(&thread_id) {
                        if let Some(turn) = thread.last_turn_mut() {
                            match result {
                                Ok(output) => {
                                    turn.record_tool_result(serde_json::to_value(output)?);
                                    drop(sess);
                                    self.call_llm(session.clone(), thread_id).await?;
                                }
                                Err(e) => {
                                    turn.record_tool_error(e.to_string());
                                    drop(sess);
                                }
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
        session: Arc<Mutex<Session>>,
        thread_id: Uuid,
    ) -> anyhow::Result<LLMResponse> {
        // Build context from thread
        let messages = {
            let sess = session.lock().await;
            sess.threads
                .get(&thread_id)
                .map(|t| t.messages())
                .unwrap_or_default()
        };

        // Convert to rig messages
        let rig_messages = to_rig_messages(&messages)?;

        // Call LLM
        let llm = self.agent.llm.llm.clone();
        let mut stream = llm
            .stream(CompletionRequest {
                model: None,
                preamble: None,
                chat_history: OneOrMany::many(rig_messages)?,
                documents: vec![],
                tools: self.convert_available_tool().await?,
                temperature: None,
                max_tokens: None,
                tool_choice: None,
                additional_params: None,
                output_schema: None,
            })
            .await?;

        // Stream to channel
        let thread_id_str = thread_id.to_string();
        self.channel.start_typing().await?;
        let draft_message_id = self.channel.start_chunk(&thread_id_str).await?;

        let mut sess = session.lock().await;
        let thread = sess.threads.get_mut(&thread_id).unwrap();
        if let Some(turn) = thread.last_turn_mut() {
            turn.record_draft_message_id(draft_message_id.clone());
        }

        while let Some(result) = stream.next().await {
            match result {
                Ok(content) => match content {
                    StreamedAssistantContent::Text(text) => {
                        debug!("Received message: {}", text);
                        self.channel
                            .send_chunk(&thread_id_str, draft_message_id.clone(), &text.text)
                            .await?;
                    }
                    StreamedAssistantContent::ToolCall {
                        tool_call,
                        internal_call_id: _,
                    } => {
                        debug!(
                            "Received ToolCall: {}, parameter: {:?}",
                            tool_call.function.name, tool_call.function.arguments
                        );
                    }
                    StreamedAssistantContent::ToolCallDelta {
                        id,
                        internal_call_id: _,
                        content,
                    } => {
                        debug!("Received ToolCallDelta: {}, parameter: {:?}", id, content);
                    }
                    StreamedAssistantContent::Reasoning(reasoning) => {
                        for content in reasoning.content {
                            match content {
                                ReasoningContent::Text { text, signature: _ } => {
                                    debug!("Received reasoning(text): {:?}", text);
                                    self.channel
                                        .send_chunk(&thread_id_str, draft_message_id.clone(), &text)
                                        .await?;
                                }
                                ReasoningContent::Encrypted(encrypted) => {
                                    debug!("Received reasoning(encrypted): {:?}", encrypted);
                                }
                                ReasoningContent::Redacted { data } => {
                                    debug!("Received reasoning(data): {:?}", data);
                                }
                                ReasoningContent::Summary(summary) => {
                                    debug!("Received reasoning(summary): {:?}", summary);
                                    self.channel
                                        .send_chunk(
                                            &thread_id_str,
                                            draft_message_id.clone(),
                                            &summary,
                                        )
                                        .await?;
                                }
                                _ => unreachable!("Unexpected content type"),
                            }
                        }
                    }
                    StreamedAssistantContent::ReasoningDelta { .. } => {}
                    StreamedAssistantContent::Final(_) => {}
                },
                Err(e) => return Err(e.into()),
            }
        }

        // Send final message
        self.channel
            .send_final(&thread_id_str, draft_message_id)
            .await?;

        Ok(LLMResponse::from(stream.choice))
    }

    async fn convert_available_tool(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let tools = self.tool_registry.all_tools().await;

        let tool_definitions = tools
            .iter()
            .map(|tool| ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            })
            .collect();
        Ok(tool_definitions)
    }
}
