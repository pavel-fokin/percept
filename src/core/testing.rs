//! Fakes and builders for the core ports - an `EventLog` - and the
//! value helpers every layer's tests share. Each fake implements one
//! core port and nothing more.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::core::{
    Actor, EdgeKind, Event, EventId, EventLog, HumanId, MapId, NodeId, NodeKind, NodeRef, Payload,
    Rules, Schema, Schemas, Source, Usage,
};
use crate::shared::Timestamp;

/// The project root `source` stamps, for a test that compares paths.
pub const ROOT: &str = "/test";

/// A stable map id for fixtures that still name maps by schema. The
/// production path mints UUIDv7; tests restore this deterministic UUID
/// so separately built events for one name fold together.
pub fn map_id(name: &str) -> MapId {
    let value = name
        .bytes()
        .fold(0xcbf29ce484222325u128, |hash, byte| {
            hash.wrapping_mul(0x100000001b3).wrapping_add(u128::from(byte))
        });
    MapId::from_uuid(uuid::Uuid::from_u128(value))
}

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

/// A `HumanId` for a test that needs one but doesn't care which - every
/// call mints a fresh one, so two calls are never mistaken for the
/// same person.
pub fn human() -> Option<HumanId> {
    Some(HumanId::new())
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

    /// Holds the events lock for `compute` and every push that follows,
    /// the same guarantee `Jsonl`'s file lock gives two processes.
    fn append_batch_computed(
        &self,
        compute: crate::core::ComputeEvents<'_>,
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

/// A node on the debates map, written by the user and cited from one
/// event, for tests that need a map with something in it.
pub fn node_added(kind: &str, name: &str) -> Event {
    node_added_by(Actor::Human(human()), kind, name)
}

/// `node_added`, committed as `actor` - for a test about who wrote a
/// node.
pub fn node_added_by(actor: Actor, kind: &str, name: &str) -> Event {
    node_added_citing(actor, kind, name, vec![EventId::new()])
}

/// `node_added`, numbered `seq` - for a test whose debates map holds
/// more than one node of `kind`, where each needs the number `apply`
/// would have minted for it: `1`, then `2`, and so on.
pub fn node_added_seq(kind: &str, name: &str, seq: u32) -> Event {
    Event::new(
        Actor::Human(human()),
        source("test"),
        None,
        node_added_payload_seq("debates", kind, name, BTreeMap::new(), vec![EventId::new()], seq),
    )
}

/// `node_added`, from a path other than `/test` - for a test that
/// folds one path's map and not another's.
pub fn node_added_at(path: &str, kind: &str, name: &str) -> Event {
    Event::new(
        Actor::Human(human()),
        source_at("test", path),
        None,
        node_added_payload("debates", kind, name, BTreeMap::new(), vec![EventId::new()]),
    )
}

/// The `NodeAdded` payload every `node_added*` builder composes: `map`,
/// `kind`, `name`, `properties`, and the `sources` it cites.
pub fn node_added_payload(
    map: &str,
    kind: &str,
    name: &str,
    properties: BTreeMap<String, String>,
    sources: Vec<EventId>,
) -> Payload {
    node_added_payload_seq(map, kind, name, properties, sources, 1)
}

/// `node_added_payload`, numbered `seq` - for a test whose map holds
/// more than one node of `kind`, where each needs the number `apply`
/// would have minted for it: `1`, then `2`, and so on.
pub fn node_added_payload_seq(
    map: &str,
    kind: &str,
    name: &str,
    properties: BTreeMap<String, String>,
    sources: Vec<EventId>,
    seq: u32,
) -> Payload {
    Payload::NodeAdded {
        map: map_id(map),
        node: NodeId::new(),
        kind: kind.to_string(),
        name: name.to_string(),
        properties,
        sources,
        seq,
    }
}

/// A `node.added` event on the debates map, citing `sources` - for a
/// test that points a row at events of its own choosing, rather than
/// the fresh id `node_added_by` mints for one no event answers to.
pub fn node_added_citing(actor: Actor, kind: &str, name: &str, sources: Vec<EventId>) -> Event {
    Event::new(
        actor,
        source("test"),
        None,
        node_added_payload("debates", kind, name, BTreeMap::new(), sources),
    )
}

/// A `node.added` event on `map`, of `kind` and `name`, with
/// `properties` and citing `sources`, written by a human at the test
/// project root - for a test that needs a node outside the debates
/// map, which `node_added_citing` always writes to.
pub fn node_added_on(
    map: &str,
    kind: &str,
    name: &str,
    properties: BTreeMap<String, String>,
    sources: Vec<EventId>,
) -> Event {
    Event::new(
        Actor::Human(human()),
        source("test"),
        None,
        node_added_payload(map, kind, name, properties, sources),
    )
}

/// A `node.changed` event for `node` on `map`, renaming to `name`
/// (kept when `None`) and setting `properties` - a human's correction,
/// for a test that compares against it.
pub fn node_changed(map: &str, node: NodeId, name: Option<&str>, properties: BTreeMap<String, String>) -> Event {
    Event::new(
        Actor::Human(human()),
        source("test"),
        None,
        Payload::NodeChanged {
            map: map_id(map),
            node,
            name: name.map(str::to_string),
            properties,
            sources: Vec::new(),
        },
    )
}

/// `event`, re-stamped as created at `at` - for a test that controls
/// the order a fold sees events in.
pub fn created_at(event: Event, at: Timestamp) -> Event {
    Event::restore(
        event.id(),
        event.actor(),
        event.source().clone(),
        event.causation_id(),
        at,
        event.payload().clone(),
    )
}

/// The node id a `node.added` event minted, so a test can point an
/// edge at it.
pub fn node_id(event: &Event) -> NodeId {
    match event.payload() {
        Payload::NodeAdded { node, .. } => *node,
        _ => panic!("expected a node.added event"),
    }
}

/// An edge on the debates map, written by the user, between two nodes
/// `node_added` minted.
pub fn edge_added(kind: &str, from: &Event, to: &Event) -> Event {
    Event::new(
        Actor::Human(human()),
        source("test"),
        None,
        Payload::EdgeAdded {
            map: map_id("debates"),
            kind: kind.to_string(),
            from: node_id(from),
            to: node_id(to),
            sources: Vec::new(),
        },
    )
}

/// The `FileCited` payload every `file_cited*` builder composes:
/// `path` (repo-relative) with `excerpt` as its text, `lines` the
/// ranged read or `None` for the whole file.
pub fn file_cited_payload(path: &str, lines: Option<(u32, u32)>, excerpt: &str) -> Payload {
    Payload::FileCited {
        path: PathBuf::from(path),
        lines,
        excerpt: excerpt.to_string(),
    }
}

/// A `file.cited` event, agent-authored from the default test source,
/// citing `path` (repo-relative) with `excerpt` as its text - `lines`
/// the ranged read or `None` for the whole file.
pub fn file_cited(path: &str, lines: Option<(u32, u32)>, excerpt: &str) -> Event {
    file_cited_citing(path, lines, excerpt, None)
}

/// `file_cited`, caused by `causation` - a re-citation of an earlier
/// `file.cited` event.
pub fn file_cited_citing(
    path: &str,
    lines: Option<(u32, u32)>,
    excerpt: &str,
    causation: Option<EventId>,
) -> Event {
    Event::new(
        Actor::Agent,
        source("test"),
        causation,
        file_cited_payload(path, lines, excerpt),
    )
}

/// A test-only map shaped like a decisions map, but not one: it exists
/// so a change to a built-in TOML never touches a map-mechanics test.
pub fn debates() -> Schema {
    Schema {
        name: "debates".to_string(),
        purpose: "what a test needs from a question-and-answer map".to_string(),
        node_kinds: vec![
            NodeKind::new("topic", ""),
            NodeKind::new(
                "claim",
                "a side taken on a topic, saying why in its `why` property",
            )
            .requiring("why")
            .with_properties(&["summary"]),
            NodeKind::new("fact", "a fact that backs a claim").with_properties(&["summary", "when"]),
            NodeKind::new("verdict", "").with_properties(&["why"]),
        ],
        edge_kinds: vec![
            EdgeKind::new("about", "", &["claim"], &["topic"]),
            EdgeKind::new("backs", "", &["fact"], &["claim"]),
            EdgeKind::new("settles", "", &["verdict"], &["topic"]),
            EdgeKind::new("replaces", "", &["verdict"], &["verdict"]),
            EdgeKind::new(
                "doubts",
                "from a topic to a verdict it puts in doubt; the verdict stands until a new \
                 one replaces it",
                &["topic"],
                &["verdict"],
            ),
        ],
        rules: Rules::default(),
    }
}

/// A test-only map shaped like a tasks map, but not one - see
/// `debates`.
pub fn chores() -> Schema {
    Schema {
        name: "chores".to_string(),
        purpose: "what a test needs from a to-do map".to_string(),
        node_kinds: vec![
            NodeKind::new("chore", "one piece of work, saying why it matters in its `why` property")
                .requiring("why")
                .with_properties(&["outcome"])
                .with_states(&["open", "done", "dropped"]),
        ],
        edge_kinds: vec![EdgeKind::new("blocks", "", &["chore"], &["chore"])],
        rules: Rules::default(),
    }
}

/// The schemas a test project has: `debates` and `chores`, neither
/// mirroring a built-in map, without touching a filesystem.
pub fn schemas() -> Schemas {
    Schemas::new(vec![debates(), chores()])
}

/// Three node kinds, coarse to fine, with an edge from the finest to
/// each of the other two - the shape a test needs when it asks which
/// of two candidates claims a node.
pub fn court() -> Schema {
    Schema {
        name: "court".to_string(),
        purpose: "test fixture".to_string(),
        node_kinds: vec![
            NodeKind::new("area", ""),
            NodeKind::new("topic", ""),
            NodeKind::new("verdict", ""),
        ],
        edge_kinds: vec![
            EdgeKind::new("within", "", &["verdict"], &["area"]),
            EdgeKind::new("settles", "", &["verdict"], &["topic"]),
        ],
        rules: Rules::default(),
    }
}

/// A `kind` edge from one node to another, both named by kind and
/// name - the one `AddEdge` every map test writes.
pub fn link(map: &mut crate::core::Map, kind: &str, from: (&str, &str), to: (&str, &str)) {
    map.apply(
        crate::core::Mutation::AddEdge {
            kind: kind.to_string(),
            from: node_ref(from.0, from.1),
            to: node_ref(to.0, to.1),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
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
            NodeKind::new("file", "a source file"),
            // Its default prefix, `f`, collides with `file`'s; `fn`
            // both avoids that and reads as the keyword it names.
            NodeKind {
                prefix: "fn".to_string(),
                properties: vec!["returns".to_string()],
                ..NodeKind::new("function", "a function or method")
            },
            NodeKind::new(
                "package",
                "an external crate a file imports, like `serde_json` - never one of this \
                 project's own modules",
            ),
        ],
        edge_kinds: vec![
            EdgeKind::new(
                "contains",
                "from a file to a symbol it defines",
                &["file"],
                &["function"],
            ),
            EdgeKind::new(
                "imports",
                "from a file to what it imports",
                &["file"],
                &["file", "package"],
            ),
        ],
        rules: Rules::default(),
    }
}
