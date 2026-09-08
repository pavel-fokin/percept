//! Fakes for the harness ports - a `Model` that replays a script, a
//! `Tool`, a `Policy`, a `Snapshot`, a `ModelCatalog`. Each implements
//! one harness port and nothing more. Value helpers shared with the
//! rest of the tests live in `crate::core::testing`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::core::EventId;
use crate::harness::{
    Chunk, Message, Modality, Model, ModelCapabilities, ModelCatalog, ModelDescriptor,
    ModelListing, ModelRequest, Policy, ReasoningEffort, ReplyStream, Snapshot, Tool, ToolOutput,
    ToolSpec, Verdict,
};

/// A chunk as a script holds it: `Err` lets a script break a reply
/// mid-stream, the way a dropped connection does.
pub type ScriptedItem = Result<Chunk, Box<dyn std::error::Error + Send + Sync>>;

/// A Model that replays one script per `reply` call, and records what
/// each request carried - how many tools, and one tag per message,
/// since `Message` doesn't clone.
pub struct Scripted {
    scripts: Mutex<VecDeque<Vec<ScriptedItem>>>,
    tool_counts: Mutex<Vec<usize>>,
    message_tags: Mutex<Vec<Vec<String>>>,
    tool_use: bool,
    reasoning_efforts: &'static [ReasoningEffort],
}

impl Scripted {
    /// Scripts of chunks that all succeed - the common case.
    pub fn new(scripts: Vec<Vec<Chunk>>, tool_use: bool) -> Self {
        let scripts = scripts
            .into_iter()
            .map(|chunks| chunks.into_iter().map(Ok).collect())
            .collect();
        Self::failing(scripts, tool_use)
    }

    /// Scripts that may carry an `Err`, for the failure paths.
    pub fn failing(scripts: Vec<Vec<ScriptedItem>>, tool_use: bool) -> Self {
        Self {
            scripts: Mutex::new(scripts.into()),
            tool_counts: Mutex::new(Vec::new()),
            message_tags: Mutex::new(Vec::new()),
            tool_use,
            reasoning_efforts: &[],
        }
    }

    /// Gives the model a reasoning-effort control supporting `efforts`,
    /// its first level the configured default.
    pub fn with_reasoning_efforts(mut self, efforts: &'static [ReasoningEffort]) -> Self {
        self.reasoning_efforts = efforts;
        self
    }

    /// How many tools each request so far carried.
    pub fn tool_counts(&self) -> Vec<usize> {
        self.tool_counts.lock().unwrap().clone()
    }

    /// The tags of the last request's messages.
    pub fn last_request(&self) -> Vec<String> {
        self.message_tags.lock().unwrap().last().cloned().unwrap()
    }
}

impl Model for Scripted {
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            input: &[Modality::Text],
            output: &[Modality::Text],
            tool_use: self.tool_use,
            reasoning_efforts: self.reasoning_efforts,
            default_reasoning_effort: self.reasoning_efforts.first().copied(),
            context_window: Some(1000),
        }
    }

    fn name(&self) -> &str {
        "scripted"
    }

    fn reply(&self, request: &ModelRequest) -> ReplyStream {
        self.tool_counts.lock().unwrap().push(request.tools.len());
        self.message_tags
            .lock()
            .unwrap()
            .push(request.messages.iter().map(tag).collect());
        let script = self.scripts.lock().unwrap().pop_front().unwrap_or_default();
        Box::pin(tokio_stream::iter(script))
    }
}

/// A message as a test can assert on it: dialogue by its own text, a
/// tool message by its shape.
fn tag(message: &Message) -> String {
    match message {
        Message::Text { content, .. } => content.clone(),
        Message::ToolCall { .. } => "<call>".to_string(),
        Message::ToolResult { .. } => "<result>".to_string(),
    }
}

/// A Tool that always succeeds with the same line.
pub struct FakeTool;

impl Tool for FakeTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "search_events",
            description: "a fake",
            parameters: "{}",
        }
    }

    fn run(&self, _arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        Ok(ToolOutput::text("ran"))
    }
}

/// A Policy that gives the same verdict to every call.
pub struct FixedPolicy(pub Verdict);

impl Policy for FixedPolicy {
    fn check(&self, _tool: &str, _arguments: &str) -> Verdict {
        self.0
    }
}

/// A Snapshot that records which prompts it was asked to save the tree
/// under and which it was asked to restore, in order.
#[derive(Default)]
pub struct FakeSnapshot {
    taken: Mutex<Vec<EventId>>,
    restored: Mutex<Vec<EventId>>,
}

impl FakeSnapshot {
    pub fn taken(&self) -> Vec<EventId> {
        self.taken.lock().unwrap().clone()
    }

    pub fn restored(&self) -> Vec<EventId> {
        self.restored.lock().unwrap().clone()
    }
}

impl Snapshot for FakeSnapshot {
    fn take(&self, prompt: EventId) -> Result<(), Box<dyn std::error::Error>> {
        self.taken.lock().unwrap().push(prompt);
        Ok(())
    }

    fn restore(&self, prompt: EventId) -> Result<(), Box<dyn std::error::Error>> {
        self.restored.lock().unwrap().push(prompt);
        Ok(())
    }
}

/// A ModelCatalog that lists whatever it was given and builds whatever
/// model was registered against a descriptor, without reaching a
/// provider. `build` errs for any descriptor it wasn't given a model
/// for. `default` lists and builds nothing, for a caller that only
/// needs `App::new` to compile.
#[derive(Default)]
pub struct FakeCatalog {
    listing: Vec<ModelDescriptor>,
    models: Vec<(ModelDescriptor, Arc<dyn Model>)>,
}

impl FakeCatalog {
    pub fn new(
        listing: Vec<ModelDescriptor>,
        models: Vec<(ModelDescriptor, Arc<dyn Model>)>,
    ) -> Self {
        Self { listing, models }
    }
}

impl ModelCatalog for FakeCatalog {
    fn list(&self) -> ModelListing {
        let listing = self.listing.clone();
        Box::pin(async move { listing })
    }

    fn build(
        &self,
        descriptor: &ModelDescriptor,
    ) -> Result<Arc<dyn Model>, Box<dyn std::error::Error>> {
        self.models
            .iter()
            .find(|(candidate, _)| candidate == descriptor)
            .map(|(_, model)| model.clone())
            .ok_or_else(|| format!("no such model: {descriptor:?}").into())
    }
}
