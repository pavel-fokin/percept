//! A cognitive map on the wire: folding one from the log, printing it
//! as JSONL so `maps show` pipes into `jq` the way `events search`
//! does, and revising it - a writer's `Mutation` checked against a
//! `Snapshot` of the log and turned into the payload that records it.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{
    map_id_for, Actor, Change, Edge, Event, EventId, EventLog, Fragment, Map, MapError, MapId,
    MapReader, Mutation, Node, NodeId, Payload, Schemas, Written,
};
use crate::store::{ids, parse_event_id};

/// `events`, cut to those whose source ran at `path` - where a reader
/// cuts the log to one path before `Map::fold`, which takes what it is
/// given.
pub fn of_path<'a>(events: &'a [Event], path: &'a Path) -> impl Iterator<Item = &'a Event> + Clone {
    events.iter().filter(move |event| event.source().path == path)
}

/// The distinct `Source.path` values `events` carries, sorted - what
/// `--all-paths` folds a map over, one path at a time.
pub fn paths(events: &[Event]) -> Vec<PathBuf> {
    let paths: BTreeSet<&Path> = events.iter().map(|event| event.source().path.as_path()).collect();
    paths.into_iter().map(Path::to_path_buf).collect()
}

/// The map `name` names, folded from those of `events` whose source
/// ran at `path`.
/// A schema with no `map.created` event yet folds empty, under a
/// placeholder identity nothing else refers to - the same "not a map
/// yet" reading `Schemas::fold_all` gives a schema it skips, so a
/// reader that asks for one map by name sees an empty map rather than
/// an error mid-session.
pub fn fold_map_at(
    schemas: &Schemas,
    name: &str,
    events: &[Event],
    path: &Path,
) -> Result<Map, Box<dyn std::error::Error>> {
    let schema = schemas.find(name)?;
    let own = of_path(events, path);
    let id = map_id_for(name, own.clone())?.unwrap_or_else(MapId::new);
    Ok(Map::fold(id, schema, own)?)
}

/// `fold_map_at` over every event in `log`.
pub fn fold_map(
    log: &dyn EventLog,
    schemas: &Schemas,
    name: &str,
    path: &Path,
) -> Result<Map, Box<dyn std::error::Error>> {
    fold_map_at(schemas, name, &log.load()?, path)
}

/// The `MapReader` over every map `Schemas` knows, each folded from the
/// log at one path.
pub struct LogMaps {
    log: Arc<dyn EventLog>,
    schemas: Arc<Schemas>,
    path: PathBuf,
}

impl LogMaps {
    pub fn new(log: Arc<dyn EventLog>, schemas: Arc<Schemas>, path: PathBuf) -> Self {
        Self { log, schemas, path }
    }
}

impl MapReader for LogMaps {
    fn read(&self, name: &str) -> Result<Map, Box<dyn std::error::Error>> {
        fold_map(self.log.as_ref(), &self.schemas, name, &self.path)
    }
}

/// One writer's view of the log, loaded once: a map folded from it,
/// and the ids it carries, so cited sources are checked against that
/// one read rather than the file per id.
pub struct Snapshot {
    map: Map,
    ids: HashSet<Uuid>,
}

impl Snapshot {
    /// Opens an existing map for a write, or mints its identity and
    /// returns the `map.created` event that must commit with the write.
    /// Called only inside an event-log computed append, so creation and
    /// the first mutation share one lock and one batch.
    pub fn for_write(
        schemas: &Schemas,
        name: &str,
        source: &crate::core::Source,
        events: Vec<crate::core::Event>,
    ) -> Result<(Option<Event>, Self), Box<dyn std::error::Error>> {
        let schema = schemas.find(name)?;
        let own = of_path(&events, &source.path);
        let existing = map_id_for(name, own.clone())?;
        let (id, created) = match existing {
            Some(id) => (id, None),
            None => {
                let id = crate::core::MapId::new();
                (id, Some(Event::map_created(id, name.to_string(), source.clone())))
            }
        };
        let map = Map::fold(id, schema, own)?;
        let ids = events.iter().map(|event| event.id().as_uuid()).collect();
        Ok((created, Self { map, ids }))
    }

    /// Each cited id as an `EventId` the log carries. An id the log
    /// lacks is an error: a typo in provenance is worse than none.
    pub fn resolve(&self, ids: &[String]) -> Result<Vec<EventId>, Box<dyn std::error::Error>> {
        ids.iter()
            .map(|id| {
                let parsed =
                    parse_event_id(id).map_err(|_| format!("{id:?} is not an event id"))?;
                if !self.ids.contains(&parsed.as_uuid()) {
                    return Err(format!("no event with id {id}").into());
                }
                Ok(parsed)
            })
            .collect()
    }

    pub fn apply(&mut self, mutation: Mutation, actor: Actor) -> Result<Payload, MapError> {
        self.map.apply(mutation, actor)
    }

    /// The map as it stands in this snapshot, for a writer that checks a
    /// change against more than `apply` enforces.
    pub fn map(&self) -> &Map {
        &self.map
    }
}

/// One change to the map `name` names, folded at `source`'s path and
/// minted and committed atomically:
/// `EventLog::append_batch_computed` hands `compute` every event already in
/// the log under its lock, `revised` checks and applies `mutation`
/// against that exact fold, and the event built from the payload it
/// returns is appended before any other writer's own computed append
/// call can run. Two writers each loading the log on their own could
/// both count the same kind's existing nodes and mint the same short
/// id; this is the seam that stops them.
#[allow(clippy::too_many_arguments)]
pub fn commit(
    log: &dyn EventLog,
    schemas: &Schemas,
    name: &str,
    source: &crate::core::Source,
    sources: &[String],
    actor: Actor,
    causation: Option<EventId>,
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<crate::core::Event, Box<dyn std::error::Error>> {
    let (name, source) = (name.to_string(), source.clone());
    let batch = log.append_batch_computed(Box::new(move |events| {
        let (created, mut snapshot) = Snapshot::for_write(schemas, &name, &source, events)?;
        let sources = snapshot.resolve(sources)?;
        let payload = snapshot.apply(mutation(sources), actor)?;
        let changed = crate::core::Event::new(actor, source, causation, payload);
        Ok(created.into_iter().chain(std::iter::once(changed)).collect())
    }))?;
    batch
        .last()
        .cloned()
        .ok_or_else(|| "map write produced no event".into())
}

/// One batch of changes to the map `name` names, folded at `source`'s
/// path and minted and committed atomically: `EventLog::append_batch_computed` hands `build` a
/// `Snapshot` folded from every event already in the log under its
/// lock, and every event it returns - both the events `build` minted
/// itself, a citation's `file.cited` among them, and the ones its own
/// `Snapshot::apply` calls produced - is appended together before any
/// other writer's own append can run. `build` errs, nothing commits: a
/// batch is checked and applied to one in-memory fold before any of it
/// reaches the log, so a later step's failure leaves no trace of the
/// steps before it.
pub fn commit_batch(
    log: &dyn EventLog,
    schemas: &Schemas,
    name: &str,
    source: &crate::core::Source,
    build: impl FnOnce(&mut Snapshot) -> Result<Vec<crate::core::Event>, Box<dyn std::error::Error>>,
) -> Result<Vec<crate::core::Event>, Box<dyn std::error::Error>> {
    let name = name.to_string();
    let source = source.clone();
    log.append_batch_computed(Box::new(move |events| {
        let (created, mut snapshot) = Snapshot::for_write(schemas, &name, &source, events)?;
        let changes = build(&mut snapshot)?;
        Ok(created.into_iter().chain(changes).collect())
    }))
}

/// Ensures every declared schema has one map identity at `source`'s
/// path. Existing identities are reused; all missing creations commit
/// under one lock, so repeating the operation adds nothing.
pub fn ensure_maps(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
    let source = source.clone();
    log.append_batch_computed(Box::new(move |events| {
        let own: Vec<&Event> = of_path(&events, &source.path).collect();
        let mut created = Vec::new();
        for schema in schemas.folded() {
            if map_id_for(&schema.name, own.iter().copied())?.is_none() {
                created.push(Event::map_created(
                    crate::core::MapId::new(),
                    schema.name.clone(),
                    source.clone(),
                ));
            }
        }
        Ok(created)
    }))
}

/// Opens a reflection, creating its map in the same batch when this is
/// the first write to that schema at this path.
pub fn start_reflection(
    log: &dyn EventLog,
    schemas: &Schemas,
    name: &str,
    source: &crate::core::Source,
) -> Result<Event, Box<dyn std::error::Error>> {
    let (name, source) = (name.to_string(), source.clone());
    let batch = log.append_batch_computed(Box::new(move |events| {
        let (created, snapshot) = Snapshot::for_write(schemas, &name, &source, events)?;
        let reflection = Event::reflection_started(snapshot.map.id(), source);
        Ok(created.into_iter().chain(std::iter::once(reflection)).collect())
    }))?;
    batch
        .last()
        .cloned()
        .ok_or_else(|| "reflection produced no event".into())
}

#[derive(Serialize)]
struct MapLine<'a> {
    id: String,
    map: &'a str,
    purpose: &'a str,
    nodes: usize,
    edges: usize,
}

/// Who added a node or edge and when. Absent on a line the caller asked
/// unstamped - `read_code`'s tree walk, whose nodes were stamped by the
/// walk that built them, not by who wrote the code or when.
#[derive(Serialize)]
struct Stamp {
    /// The same object shape an event's `actor` carries on the wire -
    /// see `store::actor_value`.
    actor: serde_json::Value,
    added_at: String,
}

impl Stamp {
    /// `Some` when `stamped`, else `None` - `#[serde(flatten)]` on an
    /// `Option` omits every field of a `None` rather than writing a
    /// null, so the caller's choice is the only branch either encoder
    /// needs.
    fn of(added: &Change, stamped: bool) -> Option<Self> {
        stamped.then(|| Self {
            actor: crate::store::actor_value(added.actor),
            added_at: added.at.to_string(),
        })
    }
}

/// `Stamp` plus who last changed a node - beside `actor`, the way the
/// node itself carries them beside its own. `NodeLine`'s form of
/// `Stamp`; an edge is never changed yet, so `encode_edge` still uses
/// `Stamp` alone.
#[derive(Serialize)]
struct NodeStamp {
    #[serde(flatten)]
    stamp: Stamp,
    changed_by: &'static str,
}

impl NodeStamp {
    /// `Some` when `stamped`, else `None` - see `Stamp::of`.
    fn of(node: &Node, stamped: bool) -> Option<Self> {
        let changed = node.changed();
        Stamp::of(node.added(), stamped).map(|stamp| Self {
            stamp,
            changed_by: changed.actor.name(),
        })
    }
}

#[derive(Serialize)]
struct NodeLine<'a> {
    node: String,
    /// This node's short id - its kind's prefix and the number minted
    /// for it, `d41` - the form `--around`, `--from`/`--to`, and a
    /// short id in `revise_map`'s arguments all resolve, alongside
    /// `kind:name`.
    id: String,
    kind: &'a str,
    name: &'a str,
    properties: &'a BTreeMap<String, String>,
    sources: Vec<String>,
    #[serde(flatten)]
    stamp: Option<NodeStamp>,
}

#[derive(Serialize)]
struct EdgeLine<'a> {
    edge: &'a str,
    from: String,
    to: String,
    sources: Vec<String>,
    #[serde(flatten)]
    stamp: Option<Stamp>,
}

/// One line naming a map and its size, for `maps list`.
pub fn encode_map(map: &Map) -> String {
    serde_json::to_string(&MapLine {
        id: map.id().as_uuid().to_string(),
        map: &map.schema().name,
        purpose: &map.schema().purpose,
        nodes: map.nodes().len(),
        edges: map.edges().len(),
    })
    .expect("MapLine always serializes")
}

#[derive(Serialize)]
struct KindLine<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    gloss: &'a str,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    requires: &'a [String],
}

impl<'a> KindLine<'a> {
    fn of_nodes(kinds: &'a [crate::core::NodeKind]) -> Vec<Self> {
        kinds
            .iter()
            .map(|kind| Self {
                name: &kind.kind,
                gloss: &kind.gloss,
                requires: &kind.requires,
            })
            .collect()
    }

    /// An edge kind carries no `requires`, so every line here reports
    /// none.
    fn of_edges(kinds: &'a [crate::core::EdgeKind]) -> Vec<Self> {
        kinds
            .iter()
            .map(|kind| Self {
                name: &kind.kind,
                gloss: &kind.gloss,
                requires: &[],
            })
            .collect()
    }
}

#[derive(Serialize)]
struct SchemaLine<'a> {
    schema: &'a str,
    purpose: &'a str,
    node_kinds: Vec<KindLine<'a>>,
    edge_kinds: Vec<KindLine<'a>>,
}

/// One line describing a map's kinds, each with the gloss it carries on
/// its `Schema` - what `read_map` returns before the fragment, so the
/// model meets `package` or `option` with its meaning attached and does
/// not guess a selector from a name alone.
pub fn encode_schema(schema: &crate::core::Schema) -> String {
    serde_json::to_string(&SchemaLine {
        schema: &schema.name,
        purpose: &schema.purpose,
        node_kinds: KindLine::of_nodes(&schema.node_kinds),
        edge_kinds: KindLine::of_edges(&schema.edge_kinds),
    })
    .expect("SchemaLine always serializes")
}

#[derive(Serialize)]
struct FragmentLine<'a> {
    map: &'a str,
    shown_nodes: usize,
    total_nodes: usize,
    shown_edges: usize,
    total_edges: usize,
    boundary_edges: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<&'a str>,
}

const NOTHING_RECORDED: &str =
    "nothing has been recorded here yet; the log may still hold what it would";

/// One line saying how much of a map a fragment shows, printed before
/// the fragment's nodes so a reader knows what the cut left out.
pub fn encode_fragment(fragment: &Fragment) -> String {
    let map = fragment.map();
    serde_json::to_string(&FragmentLine {
        map: &map.schema().name,
        shown_nodes: map.nodes().len(),
        total_nodes: fragment.total_nodes(),
        shown_edges: map.edges().len(),
        total_edges: fragment.total_edges(),
        boundary_edges: fragment.boundary_edges(),
        note: (fragment.total_nodes() == 0).then_some(NOTHING_RECORDED),
    })
    .expect("FragmentLine always serializes")
}

/// A map as JSONL: every node, then every edge - the order `maps show`
/// prints and `read_map` returns. `stamped` is `false` only for
/// `read_code`'s tree walk - see `Stamp::of`.
pub fn encode_lines(map: &Map, stamped: bool) -> impl Iterator<Item = String> + '_ {
    let nodes = map.nodes().iter().map(move |node| encode_node(map, node, stamped));
    let edges = map.edges().iter().map(move |edge| encode_edge(map, edge, stamped));
    nodes.chain(edges)
}

/// A node the way a tool call names it - `{kind, name}`, matching
/// `NodeRef`, or a bare short id string, `d41`, the way this map's own
/// render shows a node. Untagged: which JSON shape the caller sent
/// decides the match. Its own type since the domain stays serde-free.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum NodeRefArgs {
    Named { kind: String, name: String },
    ShortId(String),
}

impl NodeRefArgs {
    /// Resolves this reference against `map` to the node id it names.
    /// `{kind, name}` and a short id both go through `Map::resolve_str`,
    /// so a `kind:name` string typed straight into a bare short id's
    /// place still works the way it always has.
    pub fn resolve(self, map: &Map) -> Result<NodeId, MapError> {
        let s = match self {
            Self::Named { kind, name } => format!("{kind}:{name}"),
            Self::ShortId(s) => s,
        };
        map.resolve_str(&s)
    }
}

pub fn encode_node(map: &Map, node: &Node, stamped: bool) -> String {
    serde_json::to_string(&NodeLine {
        node: node.id.as_uuid().to_string(),
        id: map.short_id(node.id).unwrap_or_default(),
        kind: &node.kind,
        name: &node.name,
        properties: &node.properties,
        sources: ids(&node.sources),
        stamp: NodeStamp::of(node, stamped),
    })
    .expect("NodeLine always serializes")
}

/// One line per edge, its ends named `kind:name` - the way `--around`
/// and `--from` take a node - so the line reads on its own instead of
/// through a join on the node lines above it.
pub fn encode_edge(map: &Map, edge: &Edge, stamped: bool) -> String {
    serde_json::to_string(&EdgeLine {
        edge: &edge.kind,
        from: node_ref(map, edge.from),
        to: node_ref(map, edge.to),
        sources: ids(&edge.sources),
        stamp: Stamp::of(edge.added(), stamped),
    })
    .expect("EdgeLine always serializes")
}

fn node_ref(map: &Map, id: NodeId) -> String {
    let node = map.node(id).expect("an edge's ends are nodes of its map");
    format!("{}:{}", node.kind, node.name)
}

#[cfg(test)]
mod tests;
