use super::{Event, Sender};

/// Identifies who authored a Message.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// One turn in a conversation - the value-object shape Model needs,
/// independent of Event's identity/audit concerns.
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Turns a conversation into a reply - the domain's core capability,
/// mechanism-agnostic. Kept synchronous for now; a network-backed
/// provider will need to move this onto a thread with a channel back to
/// the main loop - a deliberate follow-up, not an oversight.
pub trait Model {
    fn reply(&self, messages: &[Message]) -> Result<String, Box<dyn std::error::Error>>;
}

/// Converts the transcript into the form Model expects.
pub fn to_messages(events: &[Event]) -> Vec<Message> {
    events
        .iter()
        .map(|e| Message {
            role: match e.sender {
                Sender::User => Role::User,
                Sender::Assistant => Role::Assistant,
            },
            content: e.content.clone(),
        })
        .collect()
}
