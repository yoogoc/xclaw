use crate::channel::{IncomingMessage, MessageStream, OutgoingResponse};

pub trait Channel: Clone {
    async fn receive(&self) -> anyhow::Result<MessageStream> {
        todo!()
    }

    async fn send(&self, msg: &IncomingMessage, response: OutgoingResponse) -> anyhow::Result<MessageStream> {
        todo!()
    }
}