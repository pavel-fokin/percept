use super::{Actor, Event, Payload};

/// One turn in a conversation - the value-object shape Model needs,
/// independent of Event's identity and audit concerns. Derived from the
/// log at the boundary, never stored.
///
/// `role` and `content` aren't read yet - Stub ignores `messages` while
/// it streams static text - but they're Model's contract, so the lint
/// is suppressed rather than the fields removed.
#[allow(dead_code)]
pub struct Message {
    pub role: Actor,
    pub content: String,
}

/// Turns a conversation into a streamed reply - the domain's core
/// capability, mechanism-agnostic. `reply` runs synchronously - call it
/// off-thread, e.g. via `spawn_blocking` - and returning means the
/// reply is complete. Mid-stream errors are out of scope - only a
/// failure before streaming starts is returned here.
pub trait Model: Send + Sync {
    fn reply(
        &self,
        messages: &[Message],
        on_chunk: &mut dyn FnMut(String),
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// Converts the transcript into the form Model expects. The `match` is
/// exhaustive: a new `Payload` variant won't compile here until it says
/// whether it maps to a turn.
pub fn to_messages(events: &[Event]) -> Vec<Message> {
    events
        .iter()
        .map(|e| match e.payload() {
            Payload::MessageReceived { content } => Message {
                role: e.actor(),
                content: content.clone(),
            },
        })
        .collect()
}
