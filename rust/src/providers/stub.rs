use std::time::Duration;

use crate::percept::{Message, Model, Role};

/// Echoes the last user message back, prefixed, after a random 0.5-1.5s
/// delay - long enough to make the async reply fetch's gap actually
/// observable.
pub struct Stub;

impl Model for Stub {
    fn reply(&self, messages: &[Message]) -> Result<String, Box<dyn std::error::Error>> {
        std::thread::sleep(Duration::from_millis(rand::random_range(500..1500)));

        let reply = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| format!("You said: {}", m.content))
            .unwrap_or_default();
        Ok(reply)
    }
}
