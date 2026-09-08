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
    NodeRef, Payload, Schema, Scope,
};
use crate::shared::Timestamp;
use crate::store::{ids, parse_event_id};

/// The map `name` names, folded from every event in `log` that falls
/// inside `scope`.
pub fn fold_map(
    log: &dyn EventLog,
    name: &str,
    scope: &Scope,
) -> Result<Map, Box<dyn std::error::Error>> {
    Ok(Map::fold(Schema::find(name)?, scope, &log.load()?)?)
}

/// The `MapReader` for every log-folded map. `main` wraps this to route
/// `code` to the working-tree walk, so `store` never depends on `code`.
pub struct LogMaps {
    log: Arc<dyn EventLog>,
    scope: Scope,
}

impl LogMaps {
    pub fn new(log: Arc<dyn EventLog>, scope: Scope) -> Self {
        Self { log, scope }
    }
}

impl MapReader for LogMaps {
    fn read(&self, name: &str) -> Result<Map, Box<dyn std::error::Error>> {
        fold_map(self.log.as_ref(), name, &self.scope)
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
        name: &str,
        scope: &Scope,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = Schema::find(name)?;
        let events = log.load()?;
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

/// One change to the map `name` names, as the payload that records
/// it: `sources` resolved, the `Mutation` built from them checked and
/// applied. The caller commits the payload under its own actor and
/// source. The load and that append are not one locked step, so two
/// writers racing to add the same name can both succeed; the next
/// fold then fails loudly.
pub fn revise(
    log: &dyn EventLog,
    name: &str,
    scope: &Scope,
    sources: &[String],
    actor: Actor,
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let mut snapshot = Snapshot::load(log, name, scope)?;
    let sources = snapshot.resolve(sources)?;
    let mutation = mutation(sources);
    // A rule for new writes only, so the options recorded before it
    // still fold: this is why it sits here and not in `Map::apply`.
    if let Mutation::AddNode {
        kind,
        name,
        properties,
        ..
    } = &mutation
    {
        let schema = snapshot.map().schema();
        if let Some(node_kind) = schema.node_kind(kind) {
            if let Some(property) = node_kind
                .requires
                .iter()
                .find(|property| !properties.contains_key(*property))
            {
                return Err(format!(
                    "{kind} {name:?} lacks its `{property}` property, which every {kind} \
                     carries: {}",
                    node_kind.gloss
                )
                .into());
            }
        }
    }
    Ok(snapshot.apply(mutation, actor)?)
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
        (!map.schema().is_derived()).then(|| Self {
            actor: actor.name(),
            added_at: added_at.to_string(),
        })
    }
}

#[derive(Serialize)]
struct NodeLine<'a> {
    node: String,
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
}

impl<'a> KindLine<'a> {
    fn of(kinds: &'a [crate::core::Kind]) -> Vec<Self> {
        kinds
            .iter()
            .map(|kind| Self {
                name: &kind.name,
                gloss: &kind.gloss,
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

/// A node named the way a writer knows it - by kind and name - matching
/// `NodeRef`, but its own type since the domain stays serde-free.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRefArgs {
    kind: String,
    name: String,
}

impl From<NodeRefArgs> for NodeRef {
    fn from(node: NodeRefArgs) -> Self {
        NodeRef {
            kind: node.kind,
            name: node.name,
        }
    }
}

pub fn encode_node(map: &Map, node: &Node) -> String {
    serde_json::to_string(&NodeLine {
        node: node.id.as_uuid().to_string(),
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
