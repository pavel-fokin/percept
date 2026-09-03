//! Fakes shared by the test modules. Each implements a `percept` port
//! and nothing more, so it sits at the domain's level and every layer
//! above can depend on it without bending the dependency direction.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::percept::{
    self, Chunk, Event, EventId, EventQuery, EventSearch, Modality, Model, ModelCapabilities,
    ModelRequest, Payload, ReplyStream, Tool, ToolOutput, ToolSpec,
};

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

impl EventSearch for FakeLog {
    fn search(&self, query: &EventQuery) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
        Ok(query.apply(self.events.lock().unwrap().clone()))
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
        }
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
