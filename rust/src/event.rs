use std::time::SystemTime;

use crate::id::Id;

#[derive(Clone, Copy, PartialEq)]
pub enum Sender {
    User,
    Assistant,
}

/// Identifies an Event.
pub type EventId = Id<Event>;

/// Records one chat message: who sent it, what it said, and when. Events
/// are the app's only record of the transcript - they're kept in memory,
/// in the app's events vec, for the life of the process.
///
/// `id` and `created_at` aren't read yet - the UI only renders `sender`
/// and `content` - but they're part of the entity per the ADR, not dead
/// weight, so the lint is suppressed rather than the fields removed.
#[allow(dead_code)]
pub struct Event {
    pub id: EventId,
    pub created_at: SystemTime,
    pub sender: Sender,
    pub content: String,
}

impl Event {
    pub fn new(sender: Sender, content: String) -> Self {
        Self {
            id: EventId::new(),
            created_at: SystemTime::now(),
            sender,
            content,
        }
    }
}

pub fn stub_assistant_reply(user_text: &str) -> String {
    format!("You said: {user_text}")
}
