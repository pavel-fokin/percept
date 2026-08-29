//! Provider-agnostic interface for chatting with an LLM. Provider-specific
//! implementations live in the `providers` module.

/// Identifies who authored a Message, independent of any app-specific
/// entity - a Model shouldn't need to know about this app's domain types.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// One turn in a conversation.
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Turns a conversation into a reply. Kept synchronous for now - a
/// provider backed by a real network call will need to move this onto a
/// thread with a channel back to the main loop so it doesn't block the
/// UI; that's a deliberate follow-up, not an oversight.
pub trait Model {
    fn reply(&self, messages: &[Message]) -> Result<String, Box<dyn std::error::Error>>;
}
