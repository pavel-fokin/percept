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
        "no map named \"tasks\"; maps are decisions, code"
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
