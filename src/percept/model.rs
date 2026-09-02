use std::error::Error;
use std::pin::Pin;

use futures_core::Stream;

use super::{Actor, Event, Payload};

/// One piece of a streaming reply. A thinking model interleaves
/// `Thought` chunks with its `Reply` text; a provider that never thinks
/// simply only ever yields `Reply`.
pub enum Chunk {
    Thought(String),
    Reply(String),
}

/// A reply as it streams in, chunk by chunk. `Send` so it can cross
/// into a spawned task; boxed and pinned because `Model::reply` is a
/// trait method and can't return `impl Stream` directly.
pub type ReplyStream =
    Pin<Box<dyn Stream<Item = Result<Chunk, Box<dyn Error + Send + Sync>>> + Send>>;

/// One turn in a conversation - the value-object shape Model needs,
/// independent of Event's identity and audit concerns. Derived from the
/// log at the boundary, never stored.
pub struct Message {
    pub role: Actor,
    pub content: String,
}

/// A kind of content a model reads or writes. Only the three the
/// current model shapes call for - a model that needs video or
/// embeddings adds its variant then.
//
// No reader yet: the tool-use step is the first consumer. Lint
// suppressed rather than the vocabulary deferred.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    Text,
    Image,
    Audio,
}

/// What a model accepts and produces. `input` and `output` list its
/// modalities; `tool_use` says whether it can call tools. A provider's
/// capabilities are static, so the modality lists are `&'static`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub input: &'static [Modality],
    pub output: &'static [Modality],
    pub tool_use: bool,
}

/// Turns a conversation into a streamed reply - the domain's core
/// capability, mechanism-agnostic. Returning the stream doesn't mean a
/// connection succeeded - a failure to connect surfaces as the
/// stream's first `Err` item, the same as a failure mid-reply.
pub trait Model: Send + Sync {
    /// What this model can accept, produce, and whether it calls tools.
    #[allow(dead_code)]
    fn capabilities(&self) -> ModelCapabilities;

    fn reply(&self, messages: &[Message]) -> ReplyStream;
}

/// Converts the transcript into the form Model expects. Only
/// `message.received` is dialogue - a tool call recorded as `ToolUsed`
/// is filtered out rather than fabricated into a turn, and so is a
/// thought recorded as `ThoughtRecorded`: it is never replayed to the
/// model as dialogue.
pub fn to_messages(events: &[Event]) -> Vec<Message> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Payload::MessageReceived { content } => Some(Message {
                role: e.actor(),
                content: content.clone(),
            }),
            Payload::ToolUsed { .. } => None,
            Payload::ThoughtRecorded { .. } => None,
            // Replayed to the model once `Message` grows tool variants
            // (next issue); filtered until then.
            Payload::ToolCalled { .. } | Payload::ToolResulted { .. } => None,
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

    #[test]
    fn a_thought_recorded_event_is_filtered_out_while_a_neighbouring_message_survives() {
        let events = vec![
            Event::message_received(Actor::User, "hi".to_string(), "tui".to_string(), None),
            Event::thought_recorded(
                Actor::Model,
                "let me think".to_string(),
                "tui".to_string(),
                None,
            ),
            Event::message_received(Actor::Model, "done".to_string(), "tui".to_string(), None),
        ];

        let messages = to_messages(&events);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "hi");
        assert_eq!(messages[1].content, "done");
    }
}
