use std::error::Error;
use std::pin::Pin;

use tokio_stream::Stream;

use super::{Actor, Event, Payload};

/// A reply as it streams in, chunk by chunk. `Send` so it can cross
/// into a spawned task; boxed and pinned because `Model::reply` is a
/// trait method and can't return `impl Stream` directly.
pub type ReplyStream =
    Pin<Box<dyn Stream<Item = Result<String, Box<dyn Error + Send + Sync>>> + Send>>;

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
/// capability, mechanism-agnostic. Returning the stream doesn't mean a
/// connection succeeded - a failure to connect surfaces as the
/// stream's first `Err` item, the same as a failure mid-reply.
pub trait Model: Send + Sync {
    fn reply(&self, messages: &[Message]) -> ReplyStream;
}

/// Converts the transcript into the form Model expects. Only
/// `message.received` is dialogue - a tool call recorded as `ToolUsed`
/// is filtered out rather than fabricated into a turn.
pub fn to_messages(events: &[Event]) -> Vec<Message> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Payload::MessageReceived { content } => Some(Message {
                role: e.actor(),
                content: content.clone(),
            }),
            Payload::ToolUsed { .. } => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_used_event_is_filtered_out_while_a_neighbouring_message_survives() {
        let events = vec![
            Event::message_received(Actor::User, "hi".to_string(), "tui".to_string(), None),
            Event::new(
                Actor::Model,
                "claude-code".to_string(),
                None,
                Payload::ToolUsed {
                    body: r#"{"tool_name":"Edit"}"#.to_string(),
                },
            ),
            Event::message_received(Actor::Model, "done".to_string(), "tui".to_string(), None),
        ];

        let messages = to_messages(&events);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "hi");
        assert_eq!(messages[1].content, "done");
    }
}
