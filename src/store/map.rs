//! A cognitive map on the wire: folding one from the log, printing it
//! as JSONL so `maps show` pipes into `jq` the way `events search`
//! does, and revising it - a writer's one `Mutation` folded, applied,
//! and appended as the `Event` that records it.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use uuid::Uuid;

use crate::percept::{Actor, Edge, Event, EventId, EventKind, EventLog, EventQuery, EventSearch};
use crate::percept::{Map, Mutation, Node, Schema};
use crate::store::event::ids;
use crate::store::parse_event_id;

/// Folds `schema`'s map from every map event in `search`.
pub fn fold_map(
    search: &dyn EventSearch,
    schema: &'static Schema,
) -> Result<Map, Box<dyn std::error::Error>> {
    let query = EventQuery {
        kinds: vec![
            EventKind::NodeAdded,
            EventKind::NodeRemoved,
            EventKind::EdgeAdded,
            EventKind::EdgeRemoved,
        ],
        ..Default::default()
    };
    let events = search.search(&query)?;
    Ok(Map::fold(schema, &events)?)
}

/// One writer's view of the log, loaded once: `schema`'s map folded
/// from it, and the ids it carries, so cited sources are checked
/// against that one read rather than the file per id.
pub struct Snapshot {
    map: Map,
    ids: HashSet<Uuid>,
}

impl Snapshot {
    pub fn load(
        log: &dyn EventLog,
        schema: &'static Schema,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let events = log.load()?;
        let map = Map::fold(schema, &events)?;
        let ids = events.iter().map(|event| event.id().as_uuid()).collect();
        Ok(Self { map, ids })
    }

    /// Each cited id as an `EventId` the log carries. A value that is
    /// not an id at all is told where ids come from: the transcript
    /// shows none, so a model that has not searched has nothing to
    /// cite and tends to invent one. An id the log lacks is an error
    /// too - a typo in provenance is worse than none.
    pub fn resolve(&self, ids: &[String]) -> Result<Vec<EventId>, Box<dyn std::error::Error>> {
        ids.iter()
            .map(|id| {
                let parsed = parse_event_id(id).map_err(|_| {
                    format!("{id:?} is not an event id; cite ids from search_events results")
                })?;
                if !self.ids.contains(&parsed.as_uuid()) {
                    return Err(format!("no event with id {id}").into());
                }
                Ok(parsed)
            })
            .collect()
    }

    pub fn map(&mut self) -> &mut Map {
        &mut self.map
    }
}

/// A shell user's one change to `schema`'s map: `sources` resolved,
/// the `Mutation` built from them applied, and its payload appended -
/// actor `user`, source `cli`, no cause. Every rule the mutation must
/// pass lives in `Map::apply`, not here. The load and the append are
/// not one locked step, so two shells racing to add the same name can
/// both succeed; the next fold then fails loudly.
pub fn revise(
    log: &dyn EventLog,
    schema: &'static Schema,
    sources: &[String],
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Event, Box<dyn std::error::Error>> {
    let mut snapshot = Snapshot::load(log, schema)?;
    let sources = snapshot.resolve(sources)?;
    let payload = snapshot.map().apply(mutation(sources))?;
    let event = Event::new(Actor::User, "cli".to_string(), None, payload);
    log.append(&event)?;
    Ok(event)
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
    use crate::percept::{NodeId, Payload};
    use crate::store::Jsonl;
    use std::fs;
    use std::path::PathBuf;

    /// A log file on a temp path, removed when the test ends - a
    /// trailing `remove_file` never runs on the failing test, which is
    /// the one whose file you'd want gone.
    struct TempLog {
        path: PathBuf,
    }

    impl TempLog {
        fn new() -> Self {
            Self {
                path: std::env::temp_dir().join(format!("percept-map-{}.jsonl", Uuid::now_v7())),
            }
        }

        fn open(&self) -> Jsonl {
            Jsonl::open(&self.path).unwrap()
        }
    }

    impl Drop for TempLog {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn decisions() -> &'static Schema {
        Schema::named("decisions").unwrap()
    }

    fn add_node(kind: &str, name: &str) -> impl FnOnce(Vec<EventId>) -> Mutation {
        let (kind, name) = (kind.to_string(), name.to_string());
        move |sources| Mutation::AddNode {
            kind,
            name,
            properties: BTreeMap::new(),
            sources,
        }
    }

    #[test]
    fn revise_appends_the_event_the_mutation_produces() {
        let temp = TempLog::new();
        let log = temp.open();

        let event = revise(&log, decisions(), &[], add_node("option", "Rust")).unwrap();

        assert!(matches!(event.payload(), Payload::NodeAdded { name, .. } if name == "Rust"));
        let map = fold_map(&log, decisions()).unwrap();
        assert!(map.find("option", "Rust").is_some());
    }

    #[test]
    fn revise_loads_before_applying_so_a_second_call_sees_the_first() {
        let temp = TempLog::new();
        let log = temp.open();
        revise(&log, decisions(), &[], add_node("option", "Rust")).unwrap();

        let err = revise(&log, decisions(), &[], add_node("option", "Rust"))
            .err()
            .unwrap();

        assert_eq!(err.to_string(), "option \"Rust\" is already in the map");
        assert_eq!(fold_map(&log, decisions()).unwrap().nodes().len(), 1);
    }

    #[test]
    fn a_refused_mutation_appends_nothing() {
        let temp = TempLog::new();
        let log = temp.open();

        assert!(revise(&log, decisions(), &[], add_node("goal", "Ship")).is_err());

        assert!(fold_map(&log, decisions()).unwrap().nodes().is_empty());
    }

    #[test]
    fn a_source_is_checked_against_the_log_once_loaded() {
        let temp = TempLog::new();
        let log = temp.open();
        let cited = Event::message_received(Actor::User, "hi".to_string(), "t".to_string(), None);
        log.append(&cited).unwrap();
        let known = cited.id().as_uuid().to_string();
        let unknown = Uuid::now_v7().to_string();

        let ok = revise(&log, decisions(), &[known], add_node("option", "Rust")).unwrap();
        let missing = revise(
            &log,
            decisions(),
            std::slice::from_ref(&unknown),
            add_node("option", "Go"),
        )
        .err()
        .unwrap();
        let junk = revise(
            &log,
            decisions(),
            &["user".to_string()],
            add_node("option", "Go"),
        )
        .err()
        .unwrap();

        assert!(matches!(ok.payload(), Payload::NodeAdded { sources, .. } if sources.len() == 1));
        assert_eq!(missing.to_string(), format!("no event with id {unknown}"));
        assert!(junk.to_string().contains("cite ids from search_events"));
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
