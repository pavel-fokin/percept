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
    ToolCall { tool: String, arguments: String },
    /// What a tool returned, in the order it followed its `ToolCall`.
    ToolResult { content: String },
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
    fn capabilities(&self) -> ModelCapabilities;

    fn reply(&self, request: &ModelRequest) -> ReplyStream;
}

/// Converts the transcript into the form Model expects. A recorded
/// thought is left out - it is never replayed as dialogue. A change to
/// a cognitive map is left out too: it is a fact about the map, not
/// something said. Everything else maps to a `Message`, tool calls from
/// another writer included, so a later turn sees what earlier ones
/// tried.
///
/// Any slice replays, including one cut mid-tool-round: a leading tool
/// result, whose call the cut left behind, is dropped. A conversation
/// opening on a result nothing asked for is not one a provider accepts.
/// A caller passing a whole log drops nothing.
pub fn to_messages(events: &[Event]) -> Vec<Message> {
    events
        .iter()
        .filter_map(|e| match e.payload() {
            Payload::MessageReceived { content } => Some(Message::Text {
                role: e.actor(),
                content: content.clone(),
            }),
            Payload::ToolCalled { tool, arguments } => Some(Message::ToolCall {
                tool: tool.clone(),
                arguments: arguments.clone(),
            }),
            Payload::ToolResulted { content } => Some(Message::ToolResult {
                content: content.clone(),
            }),
            Payload::ThoughtRecorded { .. }
            | Payload::NodeAdded { .. }
            | Payload::NodeRemoved { .. }
            | Payload::EdgeAdded { .. }
            | Payload::EdgeRemoved { .. } => None,
        })
        .skip_while(|message| matches!(message, Message::ToolResult { .. }))
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
    fn a_map_change_is_filtered_out_while_a_neighbouring_message_survives() {
        use crate::percept::{EventId, NodeId};
        use std::collections::BTreeMap;

        let node = NodeId::new();
        let events = vec![
            Event::message_received(Actor::User, "hi".to_string(), "tui".to_string(), None),
            Event::new(
                Actor::System,
                "tui".to_string(),
                None,
                Payload::NodeAdded {
                    map: "decisions".to_string(),
                    node,
                    kind: "evidence".to_string(),
                    name: "Both built in parallel".to_string(),
                    properties: BTreeMap::new(),
                    sources: vec![EventId::new()],
                },
            ),
            Event::new(
                Actor::System,
                "tui".to_string(),
                None,
                Payload::EdgeAdded {
                    map: "decisions".to_string(),
                    kind: "supports".to_string(),
                    from: node,
                    to: node,
                    sources: Vec::new(),
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
    fn a_tool_call_and_its_result_replay_as_tool_messages() {
        let events = vec![
            Event::message_received(Actor::User, "search".to_string(), "tui".to_string(), None),
            Event::new(
                Actor::Model,
                "tui".to_string(),
                None,
                Payload::ToolCalled {
                    tool: "search_events".to_string(),
                    arguments: r#"{"size":5}"#.to_string(),
                },
            ),
            Event::new(
                Actor::System,
                "tui".to_string(),
                None,
                Payload::ToolResulted {
                    content: "3 events".to_string(),
                },
            ),
        ];

        let messages = to_messages(&events);

        assert_eq!(messages.len(), 3);
        assert!(matches!(messages[0], Message::Text { .. }));
        match &messages[1] {
            Message::ToolCall { tool, arguments } => {
                assert_eq!(tool, "search_events");
                assert_eq!(arguments, r#"{"size":5}"#);
            }
            _ => panic!("expected a ToolCall message"),
        }
        match &messages[2] {
            Message::ToolResult { content } => assert_eq!(content, "3 events"),
            _ => panic!("expected a ToolResult message"),
        }
    }

    #[test]
    fn a_slice_opening_on_a_tool_result_drops_it() {
        let events = vec![
            Event::new(
                Actor::System,
                "tui".to_string(),
                None,
                Payload::ToolResulted {
                    content: "3 events".to_string(),
                },
            ),
            Event::message_received(
                Actor::Model,
                "found it".to_string(),
                "tui".to_string(),
                None,
            ),
        ];

        let messages = to_messages(&events);

        assert_eq!(messages.len(), 1);
        assert_eq!(text(&messages[0]), "found it");
    }
}
