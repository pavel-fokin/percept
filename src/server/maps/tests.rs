use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::core::testing::{human, FakeLog, Fixture};
use crate::core::{Actor, Event, NodeId, Payload, Source};

/// A schema with three kinds - `concept`, `question`, `fact` - and a
/// two-hop chain of edges between them: `concept` -about-> `question`
/// -backs-> `fact`. Every edge runs from the parent to the child, so
/// `concept` heads the chain - no edge reaches it - and `fact` hangs
/// deepest. What every test here folds its map from.
const SCHEMA: &str = "\
purpose = \"test\"\n\
\n\
[nodes.concept]\n\
\n\
[nodes.question]\n\
\n\
[nodes.fact]\n\
\n\
[edges.about]\n\
from = \"concept\"\n\
to = \"question\"\n\
\n\
[edges.backs]\n\
from = \"question\"\n\
to = \"fact\"\n";

fn source_at(path: &Path) -> Source {
    Source {
        name: "agent".to_string(),
        path: path.to_path_buf(),
    }
}

fn node_added(path: &Path, kind: &str, name: &str) -> (Event, NodeId) {
    let node = NodeId::new();
    let event = Event::new(
        Actor::Human(human()),
        source_at(path),
        None,
        Payload::NodeAdded {
            map: crate::core::testing::map_id("decisions"),
            node,
            kind: kind.to_string(),
            name: name.to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
            seq: 1,
        },
    );
    (event, node)
}

fn edge_added(path: &Path, kind: &str, from: NodeId, to: NodeId) -> Event {
    Event::new(
        Actor::Human(human()),
        source_at(path),
        None,
        Payload::EdgeAdded {
            map: crate::core::testing::map_id("decisions"),
            kind: kind.to_string(),
            from,
            to,
            sources: Vec::new(),
        },
    )
}

fn params(root: &Path) -> Params {
    Params {
        root: Some(root.to_string_lossy().to_string()),
    }
}

/// A fixture with the chain schema and its three nodes and two edges
/// seeded, for a test that only cares about folding an already-built
/// map. Returns the fixture and the log so a test's `Fixture` isn't
/// dropped before `get` reads its schema file.
fn chain() -> (Fixture, FakeLog) {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/decisions.toml", SCHEMA);
    let (concept, concept_id) = node_added(fixture.path(), "concept", "the rule");
    let (question, question_id) = node_added(fixture.path(), "question", "why the rule?");
    let (fact, fact_id) = node_added(fixture.path(), "fact", "precedent");
    let about = edge_added(fixture.path(), "about", concept_id, question_id);
    let backs = edge_added(fixture.path(), "backs", question_id, fact_id);
    let created = Event::map_created(
        crate::core::testing::map_id("decisions"),
        "decisions".to_string(),
        source_at(fixture.path()),
    );
    let log = FakeLog::seeded(vec![created, concept, question, fact, about, backs]);
    (fixture, log)
}

/// The `decisions` map's id, as a string, for callers of `get`.
fn decisions_id() -> String {
    crate::core::testing::map_id("decisions").as_uuid().to_string()
}

#[test]
fn the_body_carries_every_node_and_edge_of_the_map() {
    let (fixture, log) = chain();

    let body = get(&log, &decisions_id(), params(fixture.path())).unwrap();

    let mut kinds: Vec<&str> = body["nodes"].as_array().unwrap().iter().map(|n| n["kind"].as_str().unwrap()).collect();
    kinds.sort_unstable();
    assert_eq!(body["map"]["id"], decisions_id());
    // Whole, not cut: the head (`concept`) and the nodes hanging under
    // it (`question`, `fact`) all come back, with both edges between
    // them.
    assert_eq!(kinds, ["concept", "fact", "question"], "{body}");
    assert_eq!(body["edges"].as_array().unwrap().len(), 2, "{body}");
}

#[test]
fn an_unknown_map_id_404s() {
    let (fixture, log) = chain();

    let err = get(&log, &crate::core::MapId::new().as_uuid().to_string(), params(fixture.path())).unwrap_err();

    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
}

#[test]
fn a_map_id_that_does_not_parse_400s() {
    let (fixture, log) = chain();

    let err = get(&log, "not-a-uuid", params(fixture.path())).unwrap_err();

    assert!(matches!(err, Error::Bad(_)), "{err:?}");
}

#[test]
fn a_missing_root_400s() {
    let log = FakeLog::seeded(Vec::new());

    let err = get(&log, &decisions_id(), Params { root: None }).unwrap_err();

    assert!(matches!(err, Error::Bad(_)), "{err:?}");
}

#[test]
fn a_root_the_log_has_no_events_for_404s() {
    let fixture = Fixture::new();
    let log = FakeLog::seeded(Vec::new());

    let err = get(&log, &decisions_id(), params(fixture.path())).unwrap_err();

    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
}
