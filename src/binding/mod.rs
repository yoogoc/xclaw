pub mod call_llm;
mod intent;
mod loop_outcome;
mod loop_type;
mod message_convert;
pub mod run_loop;

use crate::agent::Agent;
use crate::binding::intent::Intent;
use crate::channel::{ChannelManager, IncomingMessage};
use crate::session::{Session, SessionManager, ThreadState};
use crate::tools::ToolRegistry;
use futures::StreamExt;
use rig::completion::CompletionModel;
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
    pub fn new(agent: Arc<Agent<M>>, channel: Arc<ChannelManager>, session_manager: Arc<SessionManager>, binding_id: impl Into<String>, tz: chrono_tz::Tz) -> Self {
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
            .resolve_thread(&self.binding_id, &message.user_id, &message.channel, message.thread_id.as_deref())
            .await;

        // Step 3: Get thread state
        let thread_state = {
            let sess = session.lock().await;
            sess.threads.get(&thread_id).map(|t| t.state).unwrap_or(ThreadState::Idle)
        };

        // Step 4: Dispatch by intent and state
        let result = match (intent.clone(), thread_state) {
            (Intent::UserInput, ThreadState::Idle) => self.process_user_input(session.clone(), thread_id, message).await,
            (Intent::UserInput, ThreadState::AwaitingApproval) => {
                // Interrupt current turn and start new one
                self.session_manager.thread_interrupt(session.clone(), thread_id).await;
                self.process_user_input(session.clone(), thread_id, message).await
            }
            (Intent::ApprovalAccept, ThreadState::AwaitingApproval) => self.process_approval(session.clone(), thread_id, true, false).await,
            (Intent::ApprovalAlways, ThreadState::AwaitingApproval) => self.process_approval(session.clone(), thread_id, true, true).await,
            (Intent::ApprovalReject, ThreadState::AwaitingApproval) => self.process_approval(session.clone(), thread_id, false, false).await,
            (Intent::Interrupt, _) => {
                self.session_manager.thread_interrupt(session.clone(), thread_id).await;
                Ok(())
            }
            _ => {
                warn!("unhandled intent: {:?}, thread state: {:?}", intent, thread_state);
                Ok(())
            } // Ignore invalid combinations
        };

        if let Err(err) = result {
            self.session_manager.thread_fail_turn(session.clone(), thread_id, &err.to_string()).await;

            let channel = self.channel.clone();
            let thread_id = message.thread_id.as_ref().map_or(thread_id.to_string(), |t| t.to_string());
            channel.send(&thread_id, &err.to_string()).await?;
        }

        Ok(())
    }

    pub async fn process_user_input(&self, session: Arc<Mutex<Session>>, thread_id: Uuid, message: &IncomingMessage) -> anyhow::Result<()> {
        // Start new turn
        self.session_manager.thread_start_turn(session.clone(), thread_id, &message.content).await;

        // Run agent loop
        self.run_loop(session, thread_id).await
    }

    pub async fn process_approval(&self, session: Arc<Mutex<Session>>, thread_id: Uuid, approved: bool, always: bool) -> anyhow::Result<()> {
        if approved {
            // Add to auto-approved if always
            if always {
                let tool_names: Vec<String> = {
                    let sess = session.lock().await;
                    sess.threads
                        .get(&thread_id)
                        .map(|t| t.pending_approvals.iter().map(|a| a.tool_name.clone()).collect())
                        .unwrap_or_default()
                };

                let persist_info = {
                    let mut sess = session.lock().await;
                    for tool_name in tool_names {
                        sess.auto_approve_tool(tool_name);
                    }
                    (sess.id.to_string(), sess.auto_approved_tools.clone())
                };
                let (session_id, tools) = persist_info;
                self.session_manager.persist_auto_approve(&session_id, &tools).await;
            }

            // Continue loop
            self.run_loop(session, thread_id).await
        } else {
            // Reject: fail turn
            self.session_manager.thread_fail_turn(session.clone(), thread_id, "Tool approval rejected").await;
            Ok(())
        }
    }
}
