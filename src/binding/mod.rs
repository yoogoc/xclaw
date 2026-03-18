mod loop_type;
mod loop_outcome;

use std::sync::Arc;
use futures::StreamExt;
use rig::completion::CompletionModel;
use crate::agent::Agent;
use crate::binding::loop_type::LoopType;
use crate::channel::{Channel, IncomingMessage};

pub struct Binding<M: CompletionModel> {
    agent: Arc<Agent<M>>,
    channel: Arc<Channel>,
}

impl<M: CompletionModel> Binding<M> {
    pub fn new(agent: Arc<Agent<M>>, channel: Arc<Channel>) -> Self {
        Self { agent, channel }
    }
}


impl<M: CompletionModel> Binding<M> {
    pub async fn start(&self) -> anyhow::Result<()> {
        // self.agent.config.max_iterations
        let mut stream = self.channel.receive().await?;

        while let Some(message) = stream.next().await {
            // get session, thread_id
            // handle_message
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
        // call run_loop
        Ok(())
    }

    // agent core loop
    pub async fn run_loop(&self, message: &LoopType) -> anyhow::Result<()> {
        // self.agent.config.max_iterations
        // find thread turn
        for _iteration in 1..=self.agent.config.max_iterations {
            // Pre-LLM call hook (cost guard, tool refresh, iteration limit nudge)

            // Call LLM

            // 处理LoopOutcome:
            // Response: 正常完成，直接return
            // Stopped: 用户中止
            // MaxIterations: 超过最大tool调用
            // NeedApproval: 需要审批，要给channel发消息
            // AutoApproval: 自动审批，要给channel发消息,然后递归运行run_loop

            // After-LLM call hook
        }
        Ok(())
    }

    pub async fn llm(&self, message: &LoopType) -> anyhow::Result<()> {
        // 1. build_prompt:
        // system message + skills + tools + history message(maybe compaction) + current message
        // send to rig llm
        // stream to channel
        Ok(())
    }
}