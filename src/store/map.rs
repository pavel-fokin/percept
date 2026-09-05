//! A cognitive map on the wire: folding one from the log, printing it
//! as JSONL so `maps show` pipes into `jq` the way `events search`
//! does, and revising it - a writer's `Mutation` checked against a
//! `Snapshot` of the log and turned into the payload that records it.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use uuid::Uuid;

use crate::percept::{
    Edge, EventId, EventLog, Map, MapError, Mutation, Node, NodeId, Payload, Schema, Scope,
};
use crate::store::event::ids;
use crate::store::parse_event_id;

/// The map `name` names, folded from every event in `log` that falls
/// inside `scope`.
pub fn fold_map(
    log: &dyn EventLog,
    name: &str,
    scope: &Scope,
) -> Result<Map, Box<dyn std::error::Error>> {
    Ok(Map::fold(Schema::find(name)?, scope, &log.load()?)?)
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

    pub fn apply(&mut self, mutation: Mutation) -> Result<Payload, MapError> {
        self.map.apply(mutation)
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
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let mut snapshot = Snapshot::load(log, name, scope)?;
    let sources = snapshot.resolve(sources)?;
    Ok(snapshot.apply(mutation(sources))?)
}

#[derive(Serialize)]
struct MapLine {
    map: &'static str,
    nodes: usize,
    edges: usize,
}

#[derive(Serialize)]
struct NodeLine<'a> {
    node: String,
    kind: &'a str,
    name: &'a str,
    properties: &'a BTreeMap<String, String>,
    sources: Vec<String>,
}

#[derive(Serialize)]
struct EdgeLine<'a> {
    edge: &'a str,
    from: String,
    to: String,
    sources: Vec<String>,
}

/// One line naming a map and its size, for `maps list`.
pub fn encode_map(map: &Map) -> String {
    serde_json::to_string(&MapLine {
        map: map.schema().name,
        nodes: map.nodes().len(),
        edges: map.edges().len(),
    })
    .expect("MapLine always serializes")
}

pub fn encode_node(node: &Node) -> String {
    serde_json::to_string(&NodeLine {
        node: node.id.as_uuid().to_string(),
        kind: &node.kind,
        name: &node.name,
        properties: &node.properties,
        sources: ids(&node.sources),
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
    })
    .expect("EdgeLine always serializes")
}

fn node_ref(map: &Map, id: NodeId) -> String {
    let node = map.node(id).expect("an edge's ends are nodes of its map");
    format!("{}:{}", node.kind, node.name)
}

#[cfg(test)]
mod tests;
