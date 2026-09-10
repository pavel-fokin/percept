//! Fakes and builders for the core ports - an `EventLog` - and the
//! value helpers every layer's tests share. Each fake implements one
//! core port and nothing more.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::core::{
    Actor, Event, EventId, EventLog, Kind, NodeId, NodeRef, Payload, Schema, Schemas, Scope,
    Settlement, Source, Usage,
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

    pub fn stop_failing(&self) {
        self.fail_append.store(false, Ordering::Relaxed);
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

    /// Holds the events lock for the whole of `compute` and the push
    /// that follows, so two threads sharing one `FakeLog` see the same
    /// ordering `Jsonl`'s file lock gives two processes.
    fn append_computed(
        &self,
        compute: Box<dyn FnOnce(Vec<Event>) -> Result<Event, Box<dyn std::error::Error>> + '_>,
    ) -> Result<Event, Box<dyn std::error::Error>> {
        if self.fail_append.load(Ordering::Relaxed) {
            return Err("append failed".into());
        }
        let mut events = self.events.lock().unwrap();
        let event = compute(events.clone())?;
        events.push(event.clone());
        Ok(event)
    }

    /// `append_computed`'s batch form: holds the events lock for
    /// `compute` and every push that follows, the same guarantee
    /// `Jsonl`'s file lock gives two processes.
    fn append_batch_computed(
        &self,
        compute: Box<dyn FnOnce(Vec<Event>) -> Result<Vec<Event>, Box<dyn std::error::Error>> + '_>,
    ) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
        if self.fail_append.load(Ordering::Relaxed) {
            return Err("append failed".into());
        }
        let mut events = self.events.lock().unwrap();
        let batch = compute(events.clone())?;
        events.extend(batch.iter().cloned());
        Ok(batch)
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
        // Left at the sentinel: nothing here has more than one node of
        // a kind, so a positional fallback and a minted one agree.
        seq: 0,
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

/// The built-in decisions schema, as `mapstore`'s embedded
/// `schemas/decisions.toml` must fold to. A fixture, not production
/// code: `core` reads no file, and production starts from that
/// embedded TOML, replaced by a project's own
/// `.percept/schemas/decisions.toml` when one exists.
pub fn decisions() -> Schema {
    Schema {
        name: "decisions".to_string(),
        purpose: "what was asked, what was chosen, and why, so a settled question is not \
                  reopened"
            .to_string(),
        node_kinds: vec![
            Kind::new("question", "a matter the project had to settle"),
            Kind::new(
                "option",
                "an alternative that was weighed and lost, saying why in its `why` property",
            )
            .requiring("why"),
            Kind::new("evidence", "a fact that supports or contradicts an option"),
            Kind::new("decision", "the choice that was made, and the grounds for it"),
        ],
        edge_kinds: vec![
            Kind::new("answers", "from an option to the question it was weighed for"),
            Kind::new("supports", "from evidence to an option it backs"),
            Kind::new("contradicts", "from evidence to an option it undercuts"),
            Kind::new("resolves", "from a decision to the question it settles"),
            Kind::new("supersedes", "from a decision to an earlier one it replaces"),
            Kind::new(
                "reopens",
                "from a question to a decision it puts in doubt; the decision stands until a \
                 new one supersedes it",
            ),
        ],
        headline_kinds: vec!["question".to_string(), "decision".to_string()],
        settlement: Some(Settlement {
            by: "decision".to_string(),
            of: "question".to_string(),
        }),
    }
}

/// The built-in tasks schema, as `mapstore`'s embedded TOML must fold
/// to - see `decisions`.
pub fn tasks() -> Schema {
    Schema {
        name: "tasks".to_string(),
        purpose: "what is left to do, why it matters, and what it waits on, so a session picks \
                  up the next item without re-deriving it"
            .to_string(),
        node_kinds: vec![
            Kind::new(
                "task",
                "one piece of work left to do, saying why it matters in its `why` property",
            )
            .requiring("why"),
            Kind::new(
                "outcome",
                "what became of a task: done with its commit, or dropped with the reason",
            ),
        ],
        edge_kinds: vec![
            Kind::new("resolves", "from an outcome to the task it settles"),
            Kind::new("blocks", "from a task to the one that must wait for it"),
            Kind::new("supersedes", "from a reworded task to the wording it replaces"),
        ],
        headline_kinds: vec!["task".to_string()],
        settlement: Some(Settlement {
            by: "outcome".to_string(),
            of: "task".to_string(),
        }),
    }
}

/// The schemas a test project has: `decisions` and `tasks`, the same
/// set `main` builds from the embedded and project TOML files, without
/// touching a filesystem.
pub fn schemas() -> Schemas {
    Schemas::new(vec![decisions(), tasks()])
}

/// A schema fixture with `file`, `function`, and `package` node kinds
/// and `contains`/`imports` edge kinds - what a test needs when it
/// exercises the shape the code map used to have, now that `code` is
/// walked fresh by `read_code` and is not one of `core`'s schemas.
pub fn files() -> Schema {
    Schema {
        name: "files".to_string(),
        purpose: "test fixture".to_string(),
        node_kinds: vec![
            Kind::new("file", "a source file"),
            // Its default prefix, `f`, collides with `file`'s; `fn`
            // both avoids that and reads as the keyword it names.
            Kind {
                prefix: "fn".to_string(),
                ..Kind::new("function", "a function or method")
            },
            Kind::new(
                "package",
                "an external crate a file imports, like `serde_json` - never one of this \
                 project's own modules",
            ),
        ],
        edge_kinds: vec![
            Kind::new("contains", "from a file to a symbol it defines"),
            Kind::new("imports", "from a file to what it imports"),
        ],
        headline_kinds: vec!["file".to_string()],
        settlement: None,
    }
}
