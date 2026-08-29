use crate::percept::{Message, Model, Role};

/// Echoes the last user message back, prefixed. Useful for exercising the
/// chat UI without a real API key or network access.
pub struct Stub;

impl Model for Stub {
    fn reply(&self, messages: &[Message]) -> Result<String, Box<dyn std::error::Error>> {
        let reply = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| format!("You said: {}", m.content))
            .unwrap_or_default();
        Ok(reply)
    }
}
