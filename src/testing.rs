//! Fakes shared by the test modules. Each implements a `percept` port
//! and nothing more, so it sits at the domain's level and every layer
//! above can depend on it without bending the dependency direction.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::percept::{
    self, Actor, Chunk, Event, EventId, Map, MapRenderer, Modality, Model, ModelCapabilities,
    ModelCatalog, ModelDescriptor, ModelListing, ModelRequest, NodeId, NodeRef, Payload,
    ReplyStream, Scope, Source, Tool, ToolOutput, ToolSpec, Usage,
};

/// The project root `source` stamps, for a test that compares paths.
pub const ROOT: &str = "/test";

/// A scratch directory a test writes files into, torn down when the
/// test ends - never the repository itself.
pub struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// Writes `content` at `path`, relative to the fixture's root,
    /// creating any directories it needs.
    pub fn write(&self, path: &str, content: &str) -> &Self {
        let full = self.dir.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
        self
    }
}

/// A `Source` for tests that don't care about the path - a fixed one
/// under `ROOT`, so a caller only names the writer.
pub fn source(name: &str) -> Source {
    source_at(name, ROOT)
}

/// The scope `source`'s events fall inside.
pub fn scope() -> Scope {
    Scope::Project(PathBuf::from(ROOT))
}

pub fn node_ref(kind: &str, name: &str) -> NodeRef {
    NodeRef {
        kind: kind.to_string(),
        name: name.to_string(),
    }
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

/// A MapRenderer that records the name of every map it was asked to
/// render, in order, so a test can assert what got rerendered without
/// touching a filesystem.
#[derive(Default)]
pub struct FakeRenderer {
    rendered: Mutex<Vec<String>>,
}

impl FakeRenderer {
    pub fn rendered(&self) -> Vec<String> {
        self.rendered.lock().unwrap().clone()
    }
}

impl MapRenderer for FakeRenderer {
    fn render(&self, map: &Map) -> Result<(), Box<dyn std::error::Error>> {
        self.rendered
            .lock()
            .unwrap()
            .push(map.schema().name.to_string());
        Ok(())
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
pub struct FixedPolicy(pub percept::Verdict);

impl percept::Policy for FixedPolicy {
    fn check(&self, _tool: &str, _arguments: &str) -> percept::Verdict {
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

impl percept::Snapshot for FakeSnapshot {
    fn take(&self, prompt: EventId) -> Result<(), Box<dyn std::error::Error>> {
        self.taken.lock().unwrap().push(prompt);
        Ok(())
    }

    fn restore(&self, prompt: EventId) -> Result<(), Box<dyn std::error::Error>> {
        self.restored.lock().unwrap().push(prompt);
        Ok(())
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

/// A node on the decisions map, written by the user and cited from one
/// event, for tests that need a map with something in it.
pub fn node_added(kind: &str, name: &str) -> Event {
    node_added_by(Actor::User, kind, name)
}

/// `node_added`, committed as `actor` - for a test about who wrote a
/// node.
pub fn node_added_by(actor: Actor, kind: &str, name: &str) -> Event {
    Event::new(actor, source("test"), None, node_added_payload(kind, name))
}

/// `node_added`, from a project other than `/test` - for a test that
/// checks a map scoped to one project skips another's.
pub fn node_added_at(path: &str, kind: &str, name: &str) -> Event {
    Event::new(
        Actor::User,
        source_at("test", path),
        None,
        node_added_payload(kind, name),
    )
}

fn node_added_payload(kind: &str, name: &str) -> Payload {
    Payload::NodeAdded {
        map: "decisions".to_string(),
        node: NodeId::new(),
        kind: kind.to_string(),
        name: name.to_string(),
        properties: BTreeMap::new(),
        sources: vec![EventId::new()],
    }
}

/// The node id a `node.added` event minted, so a test can point an
/// edge at it.
pub fn node_id(event: &Event) -> NodeId {
    match event.payload() {
        Payload::NodeAdded { node, .. } => *node,
        _ => panic!("expected a node.added event"),
    }
}

/// An edge on the decisions map, written by the user, between two nodes
/// `node_added` minted.
pub fn edge_added(kind: &str, from: &Event, to: &Event) -> Event {
    Event::new(
        Actor::User,
        source("test"),
        None,
        Payload::EdgeAdded {
            map: "decisions".to_string(),
            kind: kind.to_string(),
            from: node_id(from),
            to: node_id(to),
            sources: Vec::new(),
        },
    )
}
