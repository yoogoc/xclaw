use crate::binding::loop_outcome::LoopOutcome;
use crate::llm::FinishReason;
use crate::session::{PendingApproval, Session, ThreadState};
use crate::tools::{ApprovalRequirement};
use rig::completion::CompletionModel;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::binding::Binding;

impl<M: CompletionModel> Binding<M> {
    // agent core loop
    pub async fn run_loop(
        &self,
        session: Arc<Mutex<Session>>,
        thread_id: Uuid,
    ) -> anyhow::Result<()> {
        loop {
            let outcome = self.run_agentic_loop(session.clone(), thread_id).await?;

            match outcome {
                LoopOutcome::Response(response) => {
                    self.session_manager
                        .thread_complete_turn(session.clone(), thread_id, response.content)
                        .await;
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
                        let mut sess = session.lock().await;
                        if let Some(thread) = sess.threads.get_mut(&thread_id) {
                            thread.state = ThreadState::AwaitingApproval;
                            thread.pending_approvals = approvals.into_iter().map(|b| *b).collect();
                            thread.updated_at = chrono::Utc::now();
                            let pa_json = serde_json::to_string(&thread.pending_approvals)
                                .unwrap_or_else(|_| "[]".to_string());

                            self.session_manager
                                .persist_thread_awaiting_approval(
                                    &thread.id.to_string(),
                                    &pa_json,
                                    &thread.updated_at.to_rfc3339(),
                                )
                                .await;
                        }
                        break;
                    }
                }

                LoopOutcome::MaxIterations => {
                    self.session_manager
                        .thread_fail_turn(session.clone(), thread_id, "Max iterations reached")
                        .await;
                    break;
                }

                LoopOutcome::Stopped => {
                    self.session_manager
                        .thread_interrupt(session.clone(), thread_id)
                        .await;
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
            debug!("Running agent iteration {}", iteration);
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
                let mut sess = session.lock().await;
                if let Some(thread) = sess.threads.get_mut(&thread_id) {
                    if let Some(turn) = thread.last_turn_mut() {
                        turn.record_tool_call(&approval.tool_name, approval.parameters.clone());
                        let idx = turn.tool_calls.len() - 1;
                        self.session_manager
                            .persist_tool_call(
                                &turn.id.to_string(),
                                idx,
                                &turn.tool_calls[idx].clone(),
                            )
                            .await;
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
                            let turn_id = turn.id.to_string();
                            let idx = turn.tool_calls.len().saturating_sub(1);
                            match result {
                                Ok(output) => {
                                    let val = serde_json::to_value(output)?;
                                    turn.record_tool_result(val.clone());
                                    drop(sess);
                                    self.session_manager
                                        .persist_tool_result(&turn_id, idx, Some(&val), None)
                                        .await;
                                    self.call_llm(session.clone(), thread_id).await?;
                                }
                                Err(e) => {
                                    let err_str = e.to_string();
                                    turn.record_tool_error(err_str.clone());
                                    drop(sess);
                                    self.session_manager
                                        .persist_tool_result(&turn_id, idx, None, Some(&err_str))
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

}