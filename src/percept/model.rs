use std::error::Error;
use std::pin::Pin;

use futures_core::Stream;

use super::{Actor, Event, Payload, ToolSpec};

/// One piece of a streaming reply. A thinking model interleaves
/// `Thought` chunks with its `Reply` text; a provider that never thinks
/// only ever yields `Reply`. A `ToolCall` ends the turn's text: the
/// caller runs the tool and asks again.
pub enum Chunk {
    Thought(String),
    Reply(String),
    /// The model asked to run `tool` with `arguments` (JSON text).
    /// Read by the `App` loop in a later issue.
    #[allow(dead_code)]
    ToolCall {
        tool: String,
        arguments: String,
    },
}

/// A reply as it streams in, chunk by chunk. `Send` so it can cross
/// into a spawned task; boxed and pinned because `Model::reply` is a
/// trait method and can't return `impl Stream` directly.
pub type ReplyStream =
    Pin<Box<dyn Stream<Item = Result<Chunk, Box<dyn Error + Send + Sync>>> + Send>>;

/// One entry in the conversation as Model sees it - the value-object
/// shape it needs, independent of Event's identity and audit concerns.
/// Derived from the log at the boundary, never stored.
pub enum Message {
    /// A turn of dialogue.
    Text { role: Actor, content: String },
    /// The model asked to run a tool. Replayed so a later turn sees
    /// what an earlier one already tried. `arguments` is JSON text.
    ToolCall {
        /// Pairs this call to its result for a provider that matches
        /// them by id; ollama matches by order and ignores it.
        #[allow(dead_code)]
        call_id: String,
        tool: String,
        arguments: String,
    },
    /// What a tool returned for the `ToolCall` with the same `call_id`.
    ToolResult {
        #[allow(dead_code)]
        call_id: String,
        content: String,
    },
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

/// Everything Model needs for one `reply`: the conversation so far and
/// the tools it may call. One struct so what a request carries can grow
/// without churning the signature.
pub struct ModelRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
}

/// Turns a conversation into a streamed reply - the domain's core
/// capability, mechanism-agnostic. Returning the stream doesn't mean a
/// connection succeeded - a failure to connect surfaces as the
/// stream's first `Err` item, the same as a failure mid-reply.
pub trait Model: Send + Sync {
    /// What this model can accept, produce, and whether it calls tools.
    /// The `App` loop reads `tool_use` in a later issue.
    #[allow(dead_code)]
    fn capabilities(&self) -> ModelCapabilities;

    fn reply(&self, request: &ModelRequest) -> ReplyStream;
}

/// Converts the transcript into the form Model expects. A recorded
/// thought is left out - it is never replayed as dialogue. `ToolUsed`
/// is left out too: it is opaque foreign activity with no call/result
/// shape the model could act on. Everything else maps to a `Message`,
/// percept's own tool activity included, so a later turn sees what
/// earlier ones tried.
pub fn to_messages(events: &[Event]) -> Vec<Message> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Payload::MessageReceived { content } => Some(Message::Text {
                role: e.actor(),
                content: content.clone(),
            }),
            Payload::ToolCalled {
                call_id,
                tool,
                arguments,
            } => Some(Message::ToolCall {
                call_id: call_id.clone(),
                tool: tool.clone(),
                arguments: arguments.clone(),
            }),
            Payload::ToolResulted { call_id, content } => Some(Message::ToolResult {
                call_id: call_id.clone(),
                content: content.clone(),
            }),
            Payload::ThoughtRecorded { .. } => None,
            Payload::ToolUsed { .. } => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(message: &Message) -> &str {
        match message {
            Message::Text { content, .. } => content,
            _ => panic!("expected a Text message"),
        }
    }

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
        assert_eq!(text(&messages[0]), "hi");
        assert_eq!(text(&messages[1]), "done");
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
        assert_eq!(text(&messages[0]), "hi");
        assert_eq!(text(&messages[1]), "done");
    }

    #[test]
    fn a_tool_call_and_its_result_replay_as_tool_messages() {
        let events = vec![
            Event::message_received(Actor::User, "search".to_string(), "tui".to_string(), None),
            Event::new(
                Actor::Model,
                "tui".to_string(),
                None,
                Payload::ToolCalled {
                    call_id: "c1".to_string(),
                    tool: "search_events".to_string(),
                    arguments: r#"{"size":5}"#.to_string(),
                },
            ),
            Event::new(
                Actor::System,
                "tui".to_string(),
                None,
                Payload::ToolResulted {
                    call_id: "c1".to_string(),
                    content: "3 events".to_string(),
                },
            ),
        ];

        let messages = to_messages(&events);

        assert_eq!(messages.len(), 3);
        assert!(matches!(messages[0], Message::Text { .. }));
        match &messages[1] {
            Message::ToolCall {
                call_id,
                tool,
                arguments,
            } => {
                assert_eq!(call_id, "c1");
                assert_eq!(tool, "search_events");
                assert_eq!(arguments, r#"{"size":5}"#);
            }
            _ => panic!("expected a ToolCall message"),
        }
        match &messages[2] {
            Message::ToolResult { call_id, content } => {
                assert_eq!(call_id, "c1");
                assert_eq!(content, "3 events");
            }
            _ => panic!("expected a ToolResult message"),
        }
    }
}
