use crate::channel::{Channel, IncomingMessage, MessageStream, OutgoingResponse};
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures::stream::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

type WsSender = mpsc::UnboundedSender<Message>;

pub struct WebSocketChannel {
    connections: Arc<RwLock<HashMap<String, WsSender>>>,
    message_rx: Arc<RwLock<mpsc::UnboundedReceiver<IncomingMessage>>>,
    message_tx: mpsc::UnboundedSender<IncomingMessage>,
}

impl WebSocketChannel {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            message_rx: Arc::new(RwLock::new(rx)),
            message_tx: tx,
        }
    }

    pub async fn handle_connection(&self, user_id: String, ws: WebSocket) {
        let (mut ws_tx, mut ws_rx) = ws.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

        // Store connection
        {
            let mut conns = self.connections.write().await;
            conns.insert(user_id.clone(), tx);
        }

        let message_tx = self.message_tx.clone();
        let user_id_clone = user_id.clone();

        // Spawn sender task
        let send_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                use futures::SinkExt;
                if ws_tx.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // Receive messages
        while let Some(Ok(msg)) = ws_rx.next().await {
            if let Message::Text(text) = msg {
                if let Ok(incoming) = self.parse_message(&user_id_clone, &text) {
                    let _ = message_tx.send(incoming);
                }
            }
        }

        // Cleanup
        {
            let mut conns = self.connections.write().await;
            conns.remove(&user_id);
        }
        send_task.abort();
    }

    fn parse_message(&self, user_id: &str, text: &str) -> Result<IncomingMessage> {
        let msg: serde_json::Value = serde_json::from_str(text)?;

        Ok(IncomingMessage {
            id: Uuid::new_v4(),
            external_id: None,
            channel: "websocket".to_string(),
            user_id: user_id.to_string(),
            user_name: None,
            content: msg["content"].as_str().unwrap_or("").to_string(),
            thread_id: msg["thread_id"].as_str().map(String::from),
            received_at: chrono::Utc::now(),
            metadata: msg.clone(),
            timezone: None,
            attachments: vec![],
        })
    }
}

#[async_trait]
impl Channel for WebSocketChannel {
    async fn receive(&self) -> Result<MessageStream> {
        let rx = self.message_rx.clone();
        let stream = async_stream::stream! {
            let mut rx = rx.write().await;
            while let Some(msg) = rx.recv().await {
                yield msg;
            }
        };
        Ok(Box::pin(stream))
    }

    async fn send(&self, response: OutgoingResponse) -> Result<()> {
        let msg = serde_json::json!({
            "type": if response.is_draft { "draft" } else { "final" },
            "content": response.content,
            "thread_id": response.thread_id,
            "is_complete": !response.is_draft,
        });

        let text = Message::Text(msg.to_string().into());

        // Send to all connections (broadcast)
        let conns = self.connections.read().await;
        for tx in conns.values() {
            let _ = tx.send(text.clone());
        }
        Ok(())
    }

    fn supports_draft_updates(&self) -> bool {
        true
    }

    fn platform(&self) -> &str {
        "websocket"
    }
}
