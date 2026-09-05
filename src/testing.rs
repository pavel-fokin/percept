//! Fakes shared by the test modules. Each implements a `percept` port
//! and nothing more, so it sits at the domain's level and every layer
//! above can depend on it without bending the dependency direction.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::percept::{
    self, Actor, Chunk, Event, EventId, Modality, Model, ModelCapabilities, ModelCatalog,
    ModelDescriptor, ModelListing, ModelRequest, NodeId, Payload, ReplyStream, Source, Tool,
    ToolOutput, ToolSpec, Usage,
};

/// A `Source` for tests that don't care about the path - a fixed one
/// under `/test`, so a caller only names the writer.
pub fn source(name: &str) -> Source {
    source_at(name, "/test")
}

/// A `Source` under `path`, for a test that needs events from more
/// than one project.
pub fn source_at(name: &str, path: &str) -> Source {
    Source {
        name: name.to_string(),
        path: PathBuf::from(path),
    }
}

/// An in-memory EventLog. `start_failing` flips `append` into an error
/// without touching the filesystem, and can be flipped mid-conversation.
#[derive(Default)]
pub struct FakeLog {
    events: Mutex<Vec<Event>>,
    fail_append: AtomicBool,
}

impl FakeLog {
    pub fn seeded(events: Vec<Event>) -> Self {
        Self {
            events: Mutex::new(events),
            ..Self::default()
        }
    }

    pub fn start_failing(&self) {
        self.fail_append.store(true, Ordering::Relaxed);
    }
}

impl percept::EventLog for FakeLog {
    fn append(&self, event: &Event) -> Result<(), Box<dyn std::error::Error>> {
        if self.fail_append.load(Ordering::Relaxed) {
            return Err("append failed".into());
        }
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }

    fn load(&self) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
        Ok(self.events.lock().unwrap().clone())
    }

    fn get(&self, id: EventId) -> Result<Option<Event>, Box<dyn std::error::Error>> {
        let events = self.events.lock().unwrap();
        Ok(events.iter().find(|event| event.id() == id).cloned())
    }
}

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
        }
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
fn tag(message: &percept::Message) -> String {
    match message {
        percept::Message::Text { content, .. } => content.clone(),
        percept::Message::ToolCall { .. } => "<call>".to_string(),
        percept::Message::ToolResult { .. } => "<result>".to_string(),
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

/// The text of a `message.received` event, for asserting on what a turn
/// committed.
pub fn content(event: &Event) -> &str {
    match event.payload() {
        Payload::MessageReceived { content } => content,
        _ => panic!("expected a message.received event"),
    }
}

/// One round trip's counts, for tests that record or replay a
/// `model.called` event and only care that it is the same one.
pub fn usage() -> Usage {
    Usage {
        model: "gpt-5".to_string(),
        input_tokens: 100,
        output_tokens: 20,
        cached_tokens: None,
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

/// A node on the decisions map, cited from one event, for tests that
/// need a map with something in it.
pub fn node_added(kind: &str, name: &str) -> Event {
    node_added_at("/test", kind, name)
}

/// `node_added`, from a project other than `/test` - for a test that
/// checks a map scoped to one project skips another's.
pub fn node_added_at(path: &str, kind: &str, name: &str) -> Event {
    Event::new(
        Actor::User,
        source_at("test", path),
        None,
        Payload::NodeAdded {
            map: "decisions".to_string(),
            node: NodeId::new(),
            kind: kind.to_string(),
            name: name.to_string(),
            properties: BTreeMap::new(),
            sources: vec![EventId::new()],
        },
    )
}
