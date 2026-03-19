use crate::channel::IncomingMessage;

pub enum LoopType {
    UserMessage(Box<IncomingMessage>),
    ApprovalAccept,
    ApprovalDiscard
}