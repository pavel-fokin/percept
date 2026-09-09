//! A cognitive map on the wire: folding one from the log, printing it
//! as JSONL so `maps show` pipes into `jq` the way `events search`
//! does, and revising it - a writer's `Mutation` checked against a
//! `Snapshot` of the log and turned into the payload that records it.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{
    Actor, Edge, EventId, EventLog, Fragment, Map, MapError, MapReader, Mutation, Node, NodeId,
    Payload, Schemas, Scope,
};
use crate::shared::Timestamp;
use crate::store::{ids, parse_event_id};

/// The map `name` names, folded from every event in `log` that falls
/// inside `scope`.
pub fn fold_map(
    log: &dyn EventLog,
    schemas: &Schemas,
    name: &str,
    scope: &Scope,
) -> Result<Map, Box<dyn std::error::Error>> {
    Ok(Map::fold(schemas.find(name)?, scope, &log.load()?)?)
}

/// The `MapReader` for every log-folded map. `main` wraps this to route
/// `code` to the working-tree walk, so `store` never depends on `code`.
pub struct LogMaps {
    log: Arc<dyn EventLog>,
    schemas: Arc<Schemas>,
    scope: Scope,
}

impl LogMaps {
    pub fn new(log: Arc<dyn EventLog>, schemas: Arc<Schemas>, scope: Scope) -> Self {
        Self {
            log,
            schemas,
            scope,
        }
    }
}

impl MapReader for LogMaps {
    fn read(&self, name: &str) -> Result<Map, Box<dyn std::error::Error>> {
        fold_map(self.log.as_ref(), &self.schemas, name, &self.scope)
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
    pub fn load(
        log: &dyn EventLog,
        schemas: &Schemas,
        name: &str,
        scope: &Scope,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_events(schemas, name, scope, log.load()?)
    }

    /// `load`, given the events already read rather than reading them
    /// itself - what `revise` and `commit` share, so a caller holding
    /// events `EventLog::append_computed` handed it under its lock
    /// folds them the same way a fresh `load` would.
    fn from_events(
        schemas: &Schemas,
        name: &str,
        scope: &Scope,
        events: Vec<crate::core::Event>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = schemas.find(name)?;
        let map = Map::fold(schema, scope, &events)?;
        let ids = events.iter().map(|event| event.id().as_uuid()).collect();
        Ok(Self { map, ids })
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

/// `commit`'s check-and-apply, given the events its closure was handed
/// by `EventLog::append_computed` under the log's lock: `sources`
/// resolved against them, the `Mutation` built from them checked and
/// applied to their fold.
fn revised(
    schemas: &Schemas,
    name: &str,
    scope: &Scope,
    events: Vec<crate::core::Event>,
    sources: &[String],
    actor: Actor,
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let mut snapshot = Snapshot::from_events(schemas, name, scope, events)?;
    let sources = snapshot.resolve(sources)?;
    let mutation = mutation(sources);
    Ok(snapshot.apply(mutation, actor)?)
}

/// One change to the map `name` names, minted and committed atomically:
/// `EventLog::append_computed` hands `compute` every event already in
/// the log under its lock, `revised` checks and applies `mutation`
/// against that exact fold, and the event built from the payload it
/// returns is appended before any other writer's own `append_computed`
/// call can run. Two writers each loading the log on their own could
/// both count the same kind's existing nodes and mint the same short
/// id; this is the seam that stops them.
pub fn commit(
    log: &dyn EventLog,
    schemas: &Schemas,
    name: &str,
    scope: &Scope,
    source: &crate::core::Source,
    sources: &[String],
    actor: Actor,
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<crate::core::Event, Box<dyn std::error::Error>> {
    let (name, scope, source) = (name.to_string(), scope.clone(), source.clone());
    log.append_computed(Box::new(move |events| {
        let payload = revised(schemas, &name, &scope, events, sources, actor, mutation)?;
        Ok(crate::core::Event::new(actor, source, None, payload))
    }))
}

#[derive(Serialize)]
struct MapLine<'a> {
    map: &'a str,
    purpose: &'a str,
    nodes: usize,
    edges: usize,
}

/// Who added a node or edge and when. Absent on a derived map's lines:
/// its nodes were stamped by the walk that built them, and a reader
/// would take that for the moment the code was written.
#[derive(Serialize)]
struct Stamp {
    actor: &'static str,
    added_at: String,
}

impl Stamp {
    fn of(map: &Map, actor: Actor, added_at: Timestamp) -> Option<Self> {
        (!map.schema().derived).then(|| Self {
            actor: actor.name(),
            added_at: added_at.to_string(),
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
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    stamp: Option<Stamp>,
}

#[derive(Serialize)]
struct EdgeLine<'a> {
    edge: &'a str,
    from: String,
    to: String,
    sources: Vec<String>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    stamp: Option<Stamp>,
}

/// One line naming a map and its size, for `maps list`.
pub fn encode_map(map: &Map) -> String {
    serde_json::to_string(&MapLine {
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
    gloss: &'a str,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    requires: &'a [String],
}

impl<'a> KindLine<'a> {
    fn of(kinds: &'a [crate::core::Kind]) -> Vec<Self> {
        kinds
            .iter()
            .map(|kind| Self {
                name: &kind.name,
                gloss: &kind.gloss,
                requires: &kind.requires,
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
        node_kinds: KindLine::of(&schema.node_kinds),
        edge_kinds: KindLine::of(&schema.edge_kinds),
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
/// prints and `read_map` returns.
pub fn encode_lines(map: &Map) -> impl Iterator<Item = String> + '_ {
    let nodes = map.nodes().iter().map(move |node| encode_node(map, node));
    let edges = map.edges().iter().map(move |edge| encode_edge(map, edge));
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

pub fn encode_node(map: &Map, node: &Node) -> String {
    serde_json::to_string(&NodeLine {
        node: node.id.as_uuid().to_string(),
        id: map.short_id(node.id).unwrap_or_default(),
        kind: &node.kind,
        name: &node.name,
        properties: &node.properties,
        sources: ids(&node.sources),
        stamp: Stamp::of(map, node.actor, node.added_at),
    })
    .expect("NodeLine always serializes")
}

/// One line per edge, its ends named `kind:name` - the way `--around`
/// and `--from` take a node - so the line reads on its own instead of
/// through a join on the node lines above it.
pub fn encode_edge(map: &Map, edge: &Edge) -> String {
    serde_json::to_string(&EdgeLine {
        edge: &edge.kind,
        from: node_ref(map, edge.from),
        to: node_ref(map, edge.to),
        sources: ids(&edge.sources),
        stamp: Stamp::of(map, edge.actor, edge.added_at),
    })
    .expect("EdgeLine always serializes")
}

fn node_ref(map: &Map, id: NodeId) -> String {
    let node = map.node(id).expect("an edge's ends are nodes of its map");
    format!("{}:{}", node.kind, node.name)
}

#[cfg(test)]
mod tests;
