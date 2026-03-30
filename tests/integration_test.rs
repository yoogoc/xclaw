use xcraw::channel::{Channel, DiscordChannel, DiscordConfig, WebSocketChannel};
use xcraw::session::SessionManager;
use std::sync::Arc;

#[tokio::test]
async fn test_session_manager() {
    let manager = SessionManager::new();

    // Test session creation
    let (session, thread_id) = manager
        .resolve_thread("test@test", "user1", "websocket", None)
        .await;

    // Verify session and thread
    let sess = session.lock().await;
    assert_eq!(sess.binding_id, "test@test");
    assert!(sess.threads.contains_key(&thread_id));
}

#[tokio::test]
async fn test_websocket_channel() {
    let channel = WebSocketChannel::new();

    // Verify platform
    assert_eq!(channel.platform(), "websocket");
    assert!(channel.supports_draft_updates());
}

#[tokio::test]
async fn test_discord_channel() {
    let config = DiscordConfig {
        token: "test_token".to_string(),
        channel_ids: vec![],
        require_mention: false,
    };

    let channel = DiscordChannel::new(config).await.unwrap();

    // Verify platform
    assert_eq!(channel.platform(), "discord");
    assert!(!channel.supports_draft_updates());
}
