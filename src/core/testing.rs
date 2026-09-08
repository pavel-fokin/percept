//! Fakes and builders for the core ports - an `EventLog`, a
//! `MapRenderer` - and the value helpers every layer's tests share.
//! Each fake implements one core port and nothing more.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::core::{
    Actor, Event, EventId, EventLog, Map, MapRenderer, NodeId, NodeRef, Payload, Scope, Source,
    Usage,
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

impl EventLog for FakeLog {
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
