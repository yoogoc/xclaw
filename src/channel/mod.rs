mod channel;
mod message;

pub use channel::*;
pub use message::{IncomingMessage, MessageStream, OutgoingResponse};

// 十分明确，一个Channel只接收一种类型的消息
pub struct ChannelManager<C: Channel> {
    inner: C,
}

impl<C: Channel> ChannelManager<C> {
    pub fn new(c: C) -> Self {
        Self { inner: c }
    }
}

impl<C: Channel> ChannelManager<C> {
    fn allow_chunk(&self) -> bool {
        self.inner.supports_draft_updates()
    }
}

impl<C: Channel> ChannelManager<C> {
    pub async fn receive(&self) -> anyhow::Result<MessageStream> {
        todo!()
    }

    pub async fn chunk_send(
        &self,
        msg: &IncomingMessage,
        response: OutgoingResponse,
    ) -> anyhow::Result<MessageStream> {
        // 兼容draft edits：
        // 如果channel支持draft edits且配置了流式发送，那么就直接发送
        // 如果不支持draft edits,就先放到本地队列，通过flush发送
        todo!()
    }

    pub async fn flush(
        &self,
        msg: &IncomingMessage,
        response: OutgoingResponse,
    ) -> anyhow::Result<MessageStream> {
        // 兼容draft edits：
        // 如果channel支持draft edits且配置了流式发送，那么就直接发送，
        // 如果不支持draft edits
        todo!()
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        todo!()
    }
}
