use crate::agent::Message;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::VecDeque;
use tokio::sync::Mutex;

#[async_trait]
pub trait Channel: Send + Sync {
    async fn receive(&self) -> Option<Message>;
    async fn send(&self, message: Message) -> Result<()>;
}

// Mock Channel for testing
pub struct MockChannel {
    name: String,
    message_queue: Mutex<VecDeque<Message>>,
}

impl MockChannel {
    pub fn new(name: String) -> Self {
        MockChannel {
            name,
            message_queue: Mutex::new(VecDeque::new()),
        }
    }

    pub async fn push_message(&self, content: String) {
        let message = Message {
            role: "user".to_string(),
            content,
            timestamp: Utc::now(),
            task_id: None,
        };
        self.message_queue.lock().await.push_back(message);
    }
}

#[async_trait]
impl Channel for MockChannel {
    async fn receive(&self) -> Option<Message> {
        self.message_queue.lock().await.pop_front()
    }

    async fn send(&self, message: Message) -> Result<()> {
        log::info!("[{}] Sending: {}", self.name, message.content);
        Ok(())
    }
}