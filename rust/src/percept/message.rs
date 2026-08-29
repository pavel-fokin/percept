use super::{Event, Sender};

/// Identifies who authored a Message.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// One turn in a conversation - the value-object shape Model needs,
/// independent of Event's identity/audit concerns.
///
/// `role` and `content` aren't read yet - Stub ignores `messages` while
/// it streams static text - but they're part of Model's contract, not
/// dead weight, so the lint is suppressed rather than the fields
/// removed.
#[allow(dead_code)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Turns a conversation into a streamed reply - the domain's core
/// capability, mechanism-agnostic. `reply` is a single blocking call
/// (run off the UI thread via `spawn_blocking`, same as before);
/// instead of returning the whole reply at once, it invokes `on_chunk`
/// for each piece as it's produced, then returns once complete.
/// Mid-stream errors are out of scope - only a failure before streaming
/// starts is returned here.
pub trait Model: Send + Sync {
    fn reply(
        &self,
        messages: &[Message],
        on_chunk: &mut dyn FnMut(String),
    ) -> Result<(), Box<dyn std::error::Error>>;
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
