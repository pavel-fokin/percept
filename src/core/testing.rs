//! Fakes and builders for the core ports - an `EventLog` - and the
//! value helpers every layer's tests share. Each fake implements one
//! core port and nothing more.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::core::{
    Actor, EdgeKind, Event, EventId, EventLog, HumanId, MapId, NodeId, NodeKind, NodeRef, Payload,
    Schema, Schemas, Source, Usage,
};
use crate::shared::Timestamp;

/// The project root `source` stamps, for a test that compares paths.
pub const ROOT: &str = "/test";

/// A `$HOME` for a test that needs a global schema's root, distinct
/// from `ROOT`.
pub const HOME: &str = "/home";

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

/// `node_added`, from a path other than `/test`, to a map named for
/// that path - a node of another project's map, for a test that folds
/// one project's map and not another's.
pub fn node_added_at(path: &str, kind: &str, name: &str) -> Event {
    Event::new(
        Actor::Human(human()),
        source_at("test", path),
        None,
        node_added_payload(path, kind, name, BTreeMap::new(), vec![EventId::new()]),
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

/// A test-only map shaped like a decisions map, but not one: it exists
/// so a change to a built-in TOML never touches a map-mechanics test.
fn node_kind(name: &str, properties: &[(&str, &[&str])]) -> NodeKind {
    NodeKind::new(
        name,
        properties
            .iter()
            .map(|(name, values)| {
                (
                    name.to_string(),
                    values.iter().map(|value| value.to_string()).collect(),
                )
            })
            .collect(),
    )
    .expect("test node kind is valid")
}

fn node_kind_with_prefix(name: &str, prefix: &str, properties: &[(&str, &[&str])]) -> NodeKind {
    NodeKind::with_prefix(
        name,
        prefix,
        properties
            .iter()
            .map(|(name, values)| {
                (
                    name.to_string(),
                    values.iter().map(|value| value.to_string()).collect(),
                )
            })
            .collect(),
    )
    .expect("test node kind is valid")
}

fn edge_kind(name: &str, from: &[&str], to: &[&str]) -> EdgeKind {
    EdgeKind::new(
        name,
        from.iter().map(|kind| kind.to_string()).collect(),
        to.iter().map(|kind| kind.to_string()).collect(),
    )
    .expect("test edge kind is valid")
}

pub fn debates() -> Schema {
    Schema::new(
        "debates",
        "what a test needs from a question-and-answer map",
        vec![
            node_kind("topic", &[]),
            node_kind("claim", &[("why", &[]), ("summary", &[])]),
            node_kind("fact", &[("summary", &[]), ("when", &[])]),
            node_kind("verdict", &[("why", &[])]),
        ],
        vec![
            edge_kind("about", &["topic"], &["claim"]),
            edge_kind("backs", &["claim"], &["fact"]),
            edge_kind("settles", &["topic"], &["verdict"]),
            edge_kind("replaces", &["verdict"], &["verdict"]),
            edge_kind("doubts", &["verdict"], &["topic"]),
        ],
    )
    .expect("the debates test schema is valid")
}

/// A test-only map shaped like a tasks map, but not one - see
/// `debates`.
pub fn chores() -> Schema {
    Schema::new(
        "chores",
        "what a test needs from a to-do map",
        vec![node_kind(
            "chore",
            &[
                ("why", &[]),
                ("outcome", &[]),
                ("state", &["open", "done", "dropped"]),
            ],
        )],
        vec![edge_kind("blocks", &["chore"], &["chore"])],
    )
    .expect("the chores test schema is valid")
}

/// A minimal `Schemas` fake - just the folded list, without touching a
/// filesystem. `mapstore::SchemaCatalog` is production's implementor.
pub struct FakeSchemas {
    schemas: Vec<Arc<Schema>>,
    globals: BTreeMap<String, PathBuf>,
}

impl FakeSchemas {
    pub fn new(schemas: Vec<Schema>) -> Self {
        Self {
            schemas: schemas.into_iter().map(Arc::new).collect(),
            globals: BTreeMap::new(),
        }
    }

    /// `new`, with each name in `names` marked global at `home` - a
    /// test's stand-in for a schema `mapstore::SchemaCatalog` would have
    /// loaded from `$HOME/.percept/schemas`.
    pub fn with_global(schemas: Vec<Schema>, names: &[&str], home: &str) -> Self {
        let globals = names.iter().map(|name| (name.to_string(), PathBuf::from(home))).collect();
        Self {
            schemas: schemas.into_iter().map(Arc::new).collect(),
            globals,
        }
    }
}

impl Schemas for FakeSchemas {
    fn folded(&self) -> &[Arc<Schema>] {
        &self.schemas
    }

    fn global_root(&self, name: &str) -> Option<&Path> {
        self.globals.get(name).map(PathBuf::as_path)
    }
}

/// The schemas a test project has: `debates` and `chores`, neither
/// mirroring a built-in map, without touching a filesystem.
pub fn schemas() -> FakeSchemas {
    FakeSchemas::new(vec![debates(), chores()])
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
    Schema::new(
        "files",
        "test fixture",
        vec![
            node_kind("file", &[]),
            // Its default prefix, `f`, collides with `file`'s; `fn`
            // both avoids that and reads as the keyword it names.
            node_kind_with_prefix("function", "fn", &[("returns", &[])]),
            node_kind("package", &[]),
        ],
        vec![
            edge_kind("contains", &["file"], &["function"]),
            edge_kind("imports", &["file"], &["file", "package"]),
        ],
    )
    .expect("the files test schema is valid")
}
