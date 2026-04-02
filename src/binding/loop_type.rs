use crate::channel::IncomingMessage;

#[allow(unused)]
pub enum LoopType {
    UserMessage(Box<IncomingMessage>),
    ApprovalAccept,
    ApprovalDiscard,
    ApprovalAlways,
    Interrupt,
}
