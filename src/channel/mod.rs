mod message;
mod channel;

pub use message::{IncomingMessage, MessageStream, OutgoingResponse};

pub struct Channel {}

impl Channel {
    pub fn new() -> Channel {
        Channel {}
    }

    pub async fn receive(&self) -> anyhow::Result<MessageStream> {
        todo!()
    }

    pub async fn send(&self, msg: &IncomingMessage, response: OutgoingResponse) -> anyhow::Result<MessageStream> {
        todo!()
    }
}

