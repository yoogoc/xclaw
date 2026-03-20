mod loop_outcome;
mod loop_type;

use crate::agent::Agent;
use crate::binding::loop_outcome::LoopOutcome;
use crate::binding::loop_type::LoopType;
use crate::channel::{Channel, ChannelManager, IncomingMessage};
use crate::llm::{FinishReason, LLMResponse};
use crate::session::PendingApproval;
use crate::tools::ApprovalRequirement;
use futures::StreamExt;
use rig::OneOrMany;
use rig::completion::{CompletionModel, CompletionRequest};
use rig::streaming::StreamedAssistantContent;
use std::sync::Arc;
use uuid::Uuid;

pub struct Binding<M: CompletionModel, C: Channel> {
    agent: Arc<Agent<M>>,
    channel: Arc<ChannelManager<C>>,

    user_tz: chrono_tz::Tz,
}

impl<M: CompletionModel, C: Channel> Binding<M, C> {
    pub fn new(agent: Arc<Agent<M>>, channel: Arc<ChannelManager<C>>, tz: chrono_tz::Tz) -> Self {
        Self {
            agent,
            channel,
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
        // parse message：UserInput,Approval,Interrupt,Compact,Clear,SwitchThread,NewThread,Heartbeat,SystemCommand
        // UserInput: process_user_input -> run_loop
        // Approval(accept): process_approval -> run_loop
        Ok(())
    }

    pub async fn process_user_input(&self, message: &IncomingMessage) -> anyhow::Result<()> {
        // 通过llm即消息硬解析判断是否需要回复消息
        let msg = LoopType::UserMessage(Box::new(message.clone()));
        self.run_loop(&msg).await
    }

    pub async fn process_approval(&self, approved: bool, always: bool) -> anyhow::Result<()> {
        // 通过llm即消息硬解析判断是否需要回复消息
        // call run_loop
        if approved {
            self.run_loop(&LoopType::ApprovalAccept).await
        } else {
            self.run_loop(&LoopType::ApprovalDiscard).await
        }
    }

    // agent core loop
    pub async fn run_loop(&self, message: &LoopType) -> anyhow::Result<()> {
        // 调用run_agentic_loop
        // 处理LoopOutcome:
        // Response: 正常完成，完成turn，直接return
        // Stopped: 用户中止
        // MaxIterations: 超过最大tool调用
        // NeedApproval: 需要审批，要给channel发消息
        // AutoApproval: 自动审批，要给channel发消息,然后递归运行run_loop
        let outcome = self.run_agentic_loop(message).await?;
        match outcome {
            LoopOutcome::Response(_) => {
                // 对channel发送flush消息
            }
            LoopOutcome::Stopped => {
                // 对channel发送stop消息
            }
            LoopOutcome::MaxIterations => {
                // 对channel发送warn消息
            }
            LoopOutcome::ToolCall { approvals, .. } => {
                if approvals.iter().all(|approval| approval.auto_approved) {
                    let msg = LoopType::ApprovalAccept;
                    // 对channel发送消息
                    self.run_loop(&msg).await?
                }
            }
        }

        Ok(())
    }

    // agent core loop
    pub async fn run_agentic_loop(&self, message: &LoopType) -> anyhow::Result<LoopOutcome> {
        // self.agent.config.max_iterations
        // find thread turn, 将1换成turn 的 current_tool_iterations
        for _iteration in 1..=self.agent.config.max_iterations {
            // Pre-LLM call hook (cost guard, tool refresh, iteration limit nudge)

            // Call LLM
            let resp = self.llm(message).await?;

            match resp.finish_reason {
                FinishReason::Stop => return Ok(LoopOutcome::Response(Box::new(resp))),
                FinishReason::Length => {
                    todo!()
                }
                FinishReason::ToolUse => {
                    let tools = self.agent.tools().await;
                    let mut approvals = vec![];
                    let mut not_found = vec![];
                    for tc in resp.tool_calls {
                        let tool_opt = tools.get(&tc.id);
                        if let Some(tool) = tool_opt {
                            let auto_approved = match tool.requires_approval(&tc.arguments) {
                                ApprovalRequirement::Never => true,
                                ApprovalRequirement::UnlessAutoApproved => {
                                    // get session auto approved
                                    true
                                }
                                ApprovalRequirement::Always => false,
                            };
                            // let display_params =
                            //     redact_params(&tc.arguments, tool.sensitive_params());
                            let approval = PendingApproval {
                                auto_approved,
                                request_id: Uuid::new_v4(),
                                tool_name: tc.name.clone(),
                                parameters: tc.arguments.clone(),
                                display_parameters: serde_json::Value::Null,
                                description: tool.description().to_string(),
                                tool_call_id: tc.id.clone(),
                                context_messages: vec![],
                                user_timezone: Some(self.user_tz.name().to_string()),
                            };
                            approvals.push(Box::new(approval));
                        } else {
                            not_found.push(tc.id);
                        }
                    }
                    return Ok(LoopOutcome::ToolCall {
                        approvals,
                        not_found,
                    });
                }
                FinishReason::ContentFilter => {
                    todo!()
                }
                FinishReason::Unknown => {
                    todo!()
                }
            }

            // After-LLM call hook
        }

        Ok(LoopOutcome::MaxIterations)
    }

    pub async fn llm(&self, message: &LoopType) -> anyhow::Result<LLMResponse> {
        // 1. build_prompt:
        // system message + skills + tools + history message(maybe compaction) + current message
        // send to rig llm
        // stream to channel
        let llm = self.agent.llm.llm.clone();
        let mut stream = llm
            .stream(CompletionRequest {
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
            })
            .await?;

        while let Some(content) = stream.next().await {
            // Pre-LLM chunk hook
            match content {
                Ok(StreamedAssistantContent::Text(text)) => {
                    debug!("{text}");
                }
                Ok(StreamedAssistantContent::ToolCall {
                    tool_call,
                    internal_call_id,
                }) => {
                    debug!("{tool_call:?}, {internal_call_id}");
                }
                Ok(StreamedAssistantContent::ToolCallDelta {
                    id,
                    internal_call_id,
                    content,
                }) => {
                    debug!("{id}, {internal_call_id}, {content:?}");
                }
                Ok(StreamedAssistantContent::Reasoning(text)) => {
                    debug!("{text:?}");
                }
                Ok(StreamedAssistantContent::ReasoningDelta { id, reasoning }) => {
                    debug!("{id:?}, {reasoning}");
                }
                Ok(StreamedAssistantContent::Final(_result)) => {
                    debug!("Final");
                }
                Err(err) => {
                    error!("{err}");
                }
            }

            // After-LLM chunk hook
        }

        Ok(LLMResponse::from(stream.choice))
    }
}
