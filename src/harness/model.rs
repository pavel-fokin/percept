use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;

use super::ToolSpec;
use crate::core::{Actor, Event, Payload, Usage};

/// One piece of a streaming reply. A thinking model interleaves
/// `Thought` chunks with its `Reply` text; a provider that never thinks
/// only ever yields `Reply`. A `ToolCall` ends the turn's text: the
/// caller runs the tool and asks again. A provider yields `Usage` once
/// per reply, and yields it BEFORE any `ToolCall` - a `ToolCall` ends
/// the round for the caller, which stops reading the stream and runs
/// the tool, so `Usage` has to arrive first or it is never seen.
pub enum Chunk {
    Thought(String),
    Reply(String),
    /// The model asked to run `tool` with `arguments` (JSON text).
    ToolCall {
        tool: String,
        arguments: String,
    },
    Usage(Usage),
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

/// A kind of content a model reads or writes. `Thought` is the content
/// of a `Chunk::Thought` - a model that never streams one leaves it out
/// of `output`. A model that needs video or embeddings adds its variant
/// then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    Text,
    #[allow(dead_code)]
    Image,
    #[allow(dead_code)]
    Audio,
    Thought,
}

/// What a model accepts and produces. `input` and `output` list its
/// modalities; `tool_use` says whether it can call tools. A provider's
/// capabilities are static, so the modality lists are `&'static`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub input: &'static [Modality],
    pub output: &'static [Modality],
    pub tool_use: bool,
    /// Reasoning levels this model supports. Empty when it has no
    /// reasoning-effort control.
    pub reasoning_efforts: &'static [ReasoningEffort],
    /// The level the configured model uses until the session selects
    /// another supported one.
    pub default_reasoning_effort: Option<ReasoningEffort>,
    /// Tokens of context the model holds, when the provider knows it.
    /// `None` when the model isn't one the provider can size - not a
    /// guess.
    pub context_window: Option<u32>,
}

/// Everything Model needs for one `reply`: the conversation so far and
/// the tools it may call. One struct so what a request carries can grow
/// without churning the signature.
pub struct ModelRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    /// The session's selected reasoning level, when the active model
    /// supports choosing one.
    pub reasoning_effort: Option<ReasoningEffort>,
}

/// Turns a conversation into a streamed reply - the domain's core
/// capability, mechanism-agnostic. Returning the stream doesn't mean a
/// connection succeeded - a failure to connect surfaces as the
/// stream's first `Err` item, the same as a failure mid-reply.
pub trait Model: Send + Sync {
    /// What this model can accept, produce, and whether it calls tools.
    fn capabilities(&self) -> ModelCapabilities;

    /// The model's own name, as the provider names it - what a reply's
    /// `Usage::model` also carries, but available before any turn asks.
    fn name(&self) -> &str;

    fn reply(&self, request: &ModelRequest) -> ReplyStream;
}

/// The providers a `Catalog` can build a model from - closed, unlike
/// `source`, which is open by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Ollama,
    OpenAi,
    Fireworks,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Provider::Ollama => "ollama",
            Provider::OpenAi => "openai",
            Provider::Fireworks => "fireworks",
        };
        f.write_str(name)
    }
}

/// The amount of reasoning a model can be asked to use. A descriptor
/// lists the levels its provider supports, while a configured model
/// still receives the application's chosen level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    pub const ALL: &[Self] = &[Self::Low, Self::Medium, Self::High];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

impl std::fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        })
    }
}

/// Names one model a catalog can build - which provider serves it, and
/// the provider's own name for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub provider: Provider,
    pub model: String,
    /// Reasoning levels the provider supports for this model. Empty
    /// when the catalog does not know of a reasoning-effort control.
    pub reasoning_efforts: &'static [ReasoningEffort],
}

/// Listed and built, so a caller can offer every model available
/// across providers without knowing how to reach any of them. `list`
/// is async because it queries a provider's server; `build` is not -
/// building a `Model` is just construction, no round trip.
pub type ModelListing = Pin<Box<dyn Future<Output = Vec<ModelDescriptor>> + Send>>;

pub trait ModelCatalog: Send + Sync {
    /// Every model available across providers. A provider a request
    /// can't reach is left out rather than failing the whole listing.
    fn list(&self) -> ModelListing;

    /// Builds the concrete `Model` `descriptor` names.
    fn build(&self, descriptor: &ModelDescriptor) -> Result<Arc<dyn Model>, Box<dyn Error>>;
}

/// The message one event replays as, in the form Model expects. A
/// recorded thought is left out - it is never replayed as dialogue. A
/// change to a cognitive map is left out too: it is a fact about the
/// map, not something said. A `model.called` event is left out the
/// same way: it is bookkeeping about a round trip, not something
/// anyone said. Everything else maps to a `Message`, tool calls from
/// another writer included, so a later turn sees what earlier ones
/// tried.
pub fn message_of(event: &Event) -> Option<Message> {
    match event.payload() {
        Payload::MessageReceived { content } => Some(Message::Text {
            role: event.actor(),
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
        | Payload::EdgeRemoved { .. }
        | Payload::ModelCalled(..) => None,
    }
}

#[cfg(test)]
mod tests;
