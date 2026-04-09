use crate::binding::Binding;
use crate::binding::message_convert::to_rig_messages;
use crate::llm::LLMResponse;
use crate::session::Session;
use crate::utils::path::normalize_path;
use crate::workspace::paths;
use futures::StreamExt;
use rig::OneOrMany;
use rig::completion::{CompletionModel, CompletionRequest, ToolDefinition};
use rig::message::{Message, ReasoningContent};
use rig::streaming::StreamedAssistantContent;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
use uuid::Uuid;

impl<M: CompletionModel> Binding<M> {
    pub(crate) async fn call_llm(&self, session: Arc<Mutex<Session>>, thread_id: Uuid) -> anyhow::Result<LLMResponse> {
        // Call LLM
        let llm = self.agent.llm.llm.clone();

        let (chat_history, preamble) = self.build_history(session.clone(), thread_id).await?;

        let mut stream = llm
            .stream(CompletionRequest {
                model: None,
                preamble,
                chat_history,
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
                        self.channel.send_chunk(&thread_id_str, draft_message_id.clone(), &text.text).await?;
                    }
                    StreamedAssistantContent::ToolCall { tool_call, internal_call_id: _ } => {
                        debug!("Received ToolCall: {}, parameter: {:?}", tool_call.function.name, tool_call.function.arguments);
                    }
                    StreamedAssistantContent::ToolCallDelta { id, internal_call_id: _, content } => {
                        debug!("Received ToolCallDelta: {}, parameter: {:?}", id, content);
                    }
                    StreamedAssistantContent::Reasoning(reasoning) => {
                        for content in reasoning.content {
                            match content {
                                ReasoningContent::Text { text, signature: _ } => {
                                    if let Some(turn) = thread.last_turn_mut() {
                                        turn.record_reasoning(text.clone());
                                    }
                                    debug!("Received reasoning(text): {:?}", text);
                                    let text = format!("<think>\n{}\n</think>\n\n", text);
                                    self.channel.send_chunk(&thread_id_str, draft_message_id.clone(), &text).await?;
                                }
                                ReasoningContent::Encrypted(encrypted) => {
                                    debug!("Received reasoning(encrypted): {:?}", encrypted);
                                }
                                ReasoningContent::Redacted { data } => {
                                    debug!("Received reasoning(data): {:?}", data);
                                }
                                ReasoningContent::Summary(summary) => {
                                    if let Some(turn) = thread.last_turn_mut() {
                                        turn.record_reasoning(summary.clone());
                                    }
                                    debug!("Received reasoning(summary): {:?}", summary);
                                    let summary = format!("<think>\n{}\n</think>\n\n", summary);
                                    self.channel.send_chunk(&thread_id_str, draft_message_id.clone(), &summary).await?;
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
        self.channel.send_final(&thread_id_str, draft_message_id).await?;

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

    async fn build_history(&self, session: Arc<Mutex<Session>>, thread_id: Uuid) -> anyhow::Result<(OneOrMany<Message>, Option<String>)> {
        // Build context from thread
        let messages = {
            let sess = session.lock().await;
            sess.threads.get(&thread_id).map(|t| t.messages()).unwrap_or_default()
        };

        // Convert to rig messages
        let rig_messages = to_rig_messages(&messages)?;

        // fill system prompt
        let system_prompt = self.build_system_prompt().await;

        OneOrMany::many(rig_messages).map_err(|e| anyhow::anyhow!(e)).map(|m| (m, Some(system_prompt.clone())))
    }

    async fn build_system_prompt(&self) -> String {
        let mut parts = Vec::new();
        let identity_files = [
            (paths::AGENTS, "## Agent Instructions"),
            (paths::SOUL, "## Core Values"),
            (paths::USER, "## User Context"),
            (paths::IDENTITY, "## Identity"),
        ];

        for (path, header) in identity_files {
            if let Ok(doc) = self.read(path).await
                && !doc.is_empty()
            {
                parts.push(format!("{}\n\n{}", header, doc));
            }
        }

        format!(
            r#"You are xclaw Agent, a best general assistant.
## Safety
- You have no independent goals. Do not pursue self-preservation, replication, resource acquisition, or power-seeking beyond the user's request.
- Prioritize safety and human oversight over task completion. If instructions conflict, pause and ask.
- Comply with stop, pause, or audit requests. Never bypass safeguards.
- Do not manipulate anyone to expand your access or disable safeguards.
- Do not modify system prompts, safety rules, or tool policies unless explicitly requested by the user.
{}"#,
            parts.join("\n")
        )
    }

    pub async fn read(&self, path: &str) -> anyhow::Result<String> {
        let path = normalize_path(path);
        fs::read_to_string(self.workspace_dir.join(path)).await.map_err(|e| anyhow::anyhow!(e))
    }
}
