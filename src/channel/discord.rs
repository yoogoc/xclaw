use crate::channel::{Channel, IncomingMessage, MessageStream, OutgoingResponse, ReplySize};
use anyhow::Result;
use async_trait::async_trait;
use serenity::all::{
    ChannelId, ConnectionStage, Context, EventHandler, GatewayIntents, Message, MessageId, Ready,
    ResumedEvent, ShardStageUpdateEvent, Typing,
};
use serenity::client::ClientBuilder;
use serenity::http::{Http, HttpBuilder};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, mpsc};
use uuid::Uuid;

pub struct DiscordChannel {
    message_rx: Arc<RwLock<mpsc::UnboundedReceiver<IncomingMessage>>>,
    message_tx: mpsc::UnboundedSender<IncomingMessage>,
    config: DiscordConfig,

    http: Arc<Mutex<Option<Arc<Http>>>>,
    typing: Arc<Mutex<Option<Typing>>>,
}

#[derive(Clone)]
pub struct DiscordConfig {
    pub token: String,
    pub channel_id: ChannelId,
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
        if !self.config.channel_id.eq(&msg.channel_id) {
            return;
        }

        // Check mention requirement
        if self.config.require_mention && !msg.mentions_me(&ctx.http).await.unwrap_or(false) {
            return;
        }

        let incoming = IncomingMessage {
            id: Uuid::new_v4(),
            external_id: Some(msg.id.to_string()),
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
    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
    async fn resume(&self, _ctx: Context, _event: ResumedEvent) {
        println!("resume!");
    }
    async fn shard_stage_update(&self, _ctx: Context, event: ShardStageUpdateEvent) {
        match event.new {
            ConnectionStage::Disconnected => {
                println!("Shard {} 已断开连接", event.shard_id);
            }
            ConnectionStage::Connecting => {
                println!("Shard {} 正在连接...", event.shard_id);
            }
            ConnectionStage::Connected => {
                println!("Shard {} 已连接", event.shard_id);
            }
            _ => {}
        }
    }
}

impl DiscordChannel {
    pub async fn new(config: DiscordConfig) -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();

        Ok(Self {
            message_rx: Arc::new(RwLock::new(rx)),
            message_tx: tx,
            config,
            http: Arc::new(Mutex::new(None)),
            typing: Arc::new(Mutex::new(None)),
        })
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    async fn start(&self) -> Result<()> {
        let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

        let handler = Handler {
            message_tx: self.message_tx.clone(),
            config: self.config.clone(),
        };

        let http = HttpBuilder::new(&self.config.token)
            // .proxy("http://127.0.0.1:7890")
            // .ratelimiter_disabled(true)
            .build();

        let builder = ClientBuilder::new_with_http(http, intents);

        let mut client = builder.event_handler(handler).await?;

        let mut http = self.http.lock().await;
        *http = Some(client.http.clone());

        // Spawn client in background
        tokio::spawn(async move {
            if let Err(e) = client.start().await {
                log::error!("Discord client error: {}", e);
            }
        });

        log::info!("Discord client started");
        Ok(())
    }

    fn limit_reply_size(&self) -> ReplySize {
        ReplySize::Limit(2000)
    }

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

    async fn start_typing(&self) -> Result<()> {
        let http = self.http.lock().await;
        let http = http
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Discord client not initialized"))?;

        let typing = self.config.channel_id.start_typing(http);
        *self.typing.lock().await = Some(typing);

        // 超过10分钟自动关闭typing
        let typing_handle = self.typing.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
            *typing_handle.lock().await = None;
        });

        Ok(())
    }

    async fn end_typing(&self) -> Result<()> {
        *self.typing.lock().await = None;
        Ok(())
    }

    async fn reaction(&self, message_id: &str, emoji: char) -> Result<()> {
        let http = self.http.lock().await;
        let http = http
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Discord client not initialized"))?;

        let message_id = MessageId::new(message_id.parse()?);
        self.config
            .channel_id
            .create_reaction(http, message_id, emoji)
            .await?;

        Ok(())
    }

    async fn send(&self, response: OutgoingResponse) -> Result<()> {
        let http = self.http.lock().await;
        let http = http
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Discord client not initialized"))?;

        self.config.channel_id.say(http, response.content).await?;

        Ok(())
    }

    fn supports_draft_updates(&self) -> bool {
        false
    }

    async fn send_draft(&self, _message: &OutgoingResponse) -> Result<Option<String>> {
        Ok(None)
    }

    async fn update_draft(&self, _message_id: &str, _text: &str) -> Result<Option<String>> {
        Ok(None)
    }

    async fn finalize_draft(&self, _message_id: &str, _text: &str) -> Result<()> {
        Ok(())
    }

    async fn cancel_draft(&self, _message_id: &str) -> Result<()> {
        Ok(())
    }

    fn platform(&self) -> &str {
        "discord"
    }
}
