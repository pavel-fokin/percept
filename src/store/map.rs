//! A cognitive map on the wire: folding one from the log, printing it
//! as JSONL so `maps show` pipes into `jq` the way `events search`
//! does, and revising it - a writer's `Mutation` checked against a
//! `Snapshot` of the log and turned into the payload that records it.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use uuid::Uuid;

use crate::percept::{Edge, EventId, EventLog, Map, MapError, Mutation, Node, Payload, Schema};
use crate::store::event::ids;
use crate::store::parse_event_id;

/// The map `name` names, folded from every event in `log`.
pub fn fold_map(log: &dyn EventLog, name: &str) -> Result<Map, Box<dyn std::error::Error>> {
    Ok(Map::fold(Schema::find(name)?, &log.load()?)?)
}

/// One writer's view of the log, loaded once: a map folded from it,
/// and the ids it carries, so cited sources are checked against that
/// one read rather than the file per id.
pub struct Snapshot {
    map: Map,
    ids: HashSet<Uuid>,
}

impl Snapshot {
    pub fn load(log: &dyn EventLog, name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = Schema::find(name)?;
        let events = log.load()?;
        let map = Map::fold(schema, &events)?;
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
    sources: &[String],
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let mut snapshot = Snapshot::load(log, name)?;
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

pub fn encode_edge(edge: &Edge) -> String {
    serde_json::to_string(&EdgeLine {
        edge: &edge.kind,
        from: edge.from.as_uuid().to_string(),
        to: edge.to.as_uuid().to_string(),
        sources: ids(&edge.sources),
    })
    .expect("EdgeLine always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Event, NodeId};
    use crate::testing::FakeLog;

    fn add_node(kind: &str, name: &str) -> impl FnOnce(Vec<EventId>) -> Mutation {
        let (kind, name) = (kind.to_string(), name.to_string());
        move |sources| Mutation::AddNode {
            kind,
            name,
            properties: BTreeMap::new(),
            sources,
        }
    }

    /// Commits what `revise` returns the way the CLI does.
    fn record(log: &FakeLog, payload: Payload) {
        log.append(&Event::new(Actor::User, "cli".to_string(), None, payload))
            .unwrap();
    }

    #[test]
    fn revise_returns_the_payload_that_records_the_mutation() {
        let log = FakeLog::default();

        let payload = revise(&log, "decisions", &[], add_node("option", "Rust")).unwrap();

        assert!(matches!(&payload, Payload::NodeAdded { name, .. } if name == "Rust"));
        record(&log, payload);
        assert!(fold_map(&log, "decisions")
            .unwrap()
            .find("option", "Rust")
            .is_some());
    }

    #[test]
    fn revise_loads_the_log_so_a_second_call_sees_the_first() {
        let log = FakeLog::default();
        let first = revise(&log, "decisions", &[], add_node("option", "Rust")).unwrap();
        record(&log, first);

        let err = revise(&log, "decisions", &[], add_node("option", "Rust"))
            .err()
            .unwrap();

        assert_eq!(err.to_string(), "option \"Rust\" is already in the map");
    }

    #[test]
    fn revising_the_code_map_is_refused() {
        let err = revise(
            &FakeLog::default(),
            "code",
            &[],
            add_node("file", "src/main.rs"),
        )
        .err()
        .unwrap();

        assert!(err.to_string().starts_with("\"code\" is derived"), "{err}");
    }

    #[test]
    fn an_unknown_map_is_an_error() {
        let err = fold_map(&FakeLog::default(), "tasks").err().unwrap();

        assert_eq!(
            err.to_string(),
            "no map named \"tasks\"; maps are decisions"
        );
    }

    #[test]
    fn a_source_is_checked_against_the_loaded_log() {
        let cited = Event::message_received(Actor::User, "hi".to_string(), "t".to_string(), None);
        let known = cited.id().as_uuid().to_string();
        let log = FakeLog::seeded(vec![cited]);
        let unknown = Uuid::now_v7().to_string();

        let ok = revise(&log, "decisions", &[known], add_node("option", "Rust")).unwrap();
        let missing = revise(
            &log,
            "decisions",
            std::slice::from_ref(&unknown),
            add_node("option", "Go"),
        )
        .err()
        .unwrap();
        let junk = revise(
            &log,
            "decisions",
            &["user".to_string()],
            add_node("option", "Go"),
        )
        .err()
        .unwrap();

        assert!(matches!(ok, Payload::NodeAdded { sources, .. } if sources.len() == 1));
        assert_eq!(missing.to_string(), format!("no event with id {unknown}"));
        assert_eq!(junk.to_string(), "\"user\" is not an event id");
    }

    #[test]
    fn a_node_line_carries_its_id_and_sources_as_uuids() {
        let node = Node {
            id: NodeId::new(),
            kind: "evidence".to_string(),
            name: "Built both".to_string(),
            properties: BTreeMap::from([("summary".to_string(), "side by side".to_string())]),
            sources: vec![EventId::new()],
        };

        let line: serde_json::Value = serde_json::from_str(&encode_node(&node)).unwrap();

        assert_eq!(line["node"], node.id.as_uuid().to_string());
        assert_eq!(line["kind"], "evidence");
        assert_eq!(line["name"], "Built both");
        assert_eq!(line["properties"]["summary"], "side by side");
        assert_eq!(line["sources"][0], node.sources[0].as_uuid().to_string());
    }

    #[test]
    fn an_edge_line_names_its_kind_under_edge() {
        let edge = Edge {
            kind: "supports".to_string(),
            from: NodeId::new(),
            to: NodeId::new(),
            sources: Vec::new(),
        };

        let line: serde_json::Value = serde_json::from_str(&encode_edge(&edge)).unwrap();

        assert_eq!(line["edge"], "supports");
        assert_eq!(line["from"], edge.from.as_uuid().to_string());
        assert_eq!(line["sources"], serde_json::json!([]));
    }
}
