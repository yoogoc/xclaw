use crate::channel::{Channel, IncomingMessage, MessageStream, OutgoingResponse};
use anyhow::Result;
use serenity::all::{ChannelId, Context, EventHandler, GatewayIntents, Message};
use serenity::Client;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

pub struct DiscordChannel {
    client: Arc<RwLock<Option<Client>>>,
    message_rx: Arc<RwLock<mpsc::UnboundedReceiver<IncomingMessage>>>,
    message_tx: mpsc::UnboundedSender<IncomingMessage>,
    config: DiscordConfig,
}

#[derive(Clone)]
pub struct DiscordConfig {
    pub token: String,
    pub channel_ids: Vec<String>,
    pub require_mention: bool,
}

struct Handler {
    message_tx: mpsc::UnboundedSender<IncomingMessage>,
    config: DiscordConfig,
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore bot messages
        if msg.author.bot {
            return;
        }

        // Check if in monitored channels
        if !self.config.channel_ids.is_empty()
            && !self.config.channel_ids.contains(&msg.channel_id.to_string()) {
            return;
        }

        // Check mention requirement
        if self.config.require_mention && !msg.mentions_me(&ctx.http).await.unwrap_or(false) {
            return;
        }

        let incoming = IncomingMessage {
            id: Uuid::new_v4(),
            channel: "discord".to_string(),
            user_id: msg.author.id.to_string(),
            user_name: Some(msg.author.name.clone()),
            content: msg.content.clone(),
            thread_id: msg.thread.as_ref().map(|t| t.id.to_string()),
            received_at: chrono::Utc::now(),
            metadata: serde_json::json!({
                "message_id": msg.id.to_string(),
                "channel_id": msg.channel_id.to_string(),
            }),
            timezone: None,
            attachments: vec![],
        };

        let _ = self.message_tx.send(incoming);
    }
}

impl DiscordChannel {
    pub async fn new(config: DiscordConfig) -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        
        Ok(Self {
            client: Arc::new(RwLock::new(None)),
            message_rx: Arc::new(RwLock::new(rx)),
            message_tx: tx,
            config,
        })
    }

    pub async fn start(&self) -> Result<()> {
        let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
        
        let handler = Handler {
            message_tx: self.message_tx.clone(),
            config: self.config.clone(),
        };

        let client = Client::builder(&self.config.token, intents)
            .event_handler(handler)
            .await?;

        info!("connected to discord");

        *self.client.write().await = Some(client);
        Ok(())
    }
}

#[async_trait]
impl Channel for DiscordChannel {
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
        let client_lock = self.client.read().await;
        let client = client_lock.as_ref().ok_or_else(|| anyhow::anyhow!("Client not started"))?;

        if let Some(thread_id) = response.thread_id {
            let channel_id = ChannelId::new(thread_id.parse()?);
            channel_id.say(&client.http, response.content).await?;
        }

        Ok(())
    }

    fn supports_draft_updates(&self) -> bool {
        false
    }

    fn platform(&self) -> &str {
        "discord"
    }
}
