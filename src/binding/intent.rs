use crate::channel::IncomingMessage;

#[derive(Debug, Clone)]
pub enum Intent {
    UserInput,
    ApprovalAccept,
    ApprovalReject,
    ApprovalAlways,
    Interrupt,
}

impl Intent {
    pub fn parse(message: &IncomingMessage) -> Self {
        let content = message.content.trim();

        // Approval responses
        if content.eq_ignore_ascii_case("yes") || content.eq_ignore_ascii_case("y") {
            return Intent::ApprovalAccept;
        }
        if content.eq_ignore_ascii_case("no") || content.eq_ignore_ascii_case("n") {
            return Intent::ApprovalReject;
        }
        if content.eq_ignore_ascii_case("always") || content.eq_ignore_ascii_case("a") {
            return Intent::ApprovalAlways;
        }

        // Interrupt
        if content.starts_with("/stop") || content.starts_with("/interrupt") {
            return Intent::Interrupt;
        }

        // Default: user input
        Intent::UserInput
    }
}
