use std::path::PathBuf;

use super::*;
use crate::core::testing::{scope, source, source_at, FakeLog};
use crate::core::{Actor, Event, NodeId, NodeRef};
use crate::shared::Timestamp;

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
    record_at(log, "/test", payload);
}

/// `record`, stamping the commit as coming from `path` - for a test
/// that revises the same map from more than one project.
fn record_at(log: &FakeLog, path: &str, payload: Payload) {
    log.append(&Event::new(
        Actor::User,
        source_at("cli", path),
        None,
        payload,
    ))
    .unwrap();
}

#[test]
fn an_option_without_a_why_is_refused_as_a_new_write() {
    let log = FakeLog::default();

    let err = revise(
        &log,
        "decisions",
        &scope(),
        &[],
        Actor::User,
        add_node("option", "SQLite"),
    )
    .err()
    .unwrap()
    .to_string();

    assert!(err.contains("why it lost"), "{err}");
}

#[test]
fn a_task_without_a_why_is_refused_as_a_new_write() {
    let log = FakeLog::default();

    let err = revise(
        &log,
        "tasks",
        &scope(),
        &[],
        Actor::User,
        add_node("task", "cancel a turn without quitting"),
    )
    .err()
    .unwrap()
    .to_string();

    assert!(err.contains("why it matters"), "{err}");
}

#[test]
fn an_option_with_a_why_is_recorded() {
    let log = FakeLog::default();
    let mutation = |sources| Mutation::AddNode {
        kind: "option".to_string(),
        name: "SQLite".to_string(),
        properties: BTreeMap::from([("why".to_string(), "one more dependency".to_string())]),
        sources,
    };

    let payload = revise(&log, "decisions", &scope(), &[], Actor::User, mutation).unwrap();

    assert!(matches!(payload, Payload::NodeAdded { .. }));
}

#[test]
fn revise_returns_the_payload_that_records_the_mutation() {
    let log = FakeLog::default();

    let payload = revise(
        &log,
        "decisions",
        &scope(),
        &[],
        Actor::User,
        add_node("decision", "Rust"),
    )
    .unwrap();

    assert!(matches!(&payload, Payload::NodeAdded { name, .. } if name == "Rust"));
    record(&log, payload);
    assert!(fold_map(&log, "decisions", &scope())
        .unwrap()
        .find("decision", "Rust")
        .is_some());
}

#[test]
fn revise_loads_the_log_so_a_second_call_sees_the_first() {
    let log = FakeLog::default();
    let first = revise(
        &log,
        "decisions",
        &scope(),
        &[],
        Actor::User,
        add_node("decision", "Rust"),
    )
    .unwrap();
    record(&log, first);

    let err = revise(
        &log,
        "decisions",
        &scope(),
        &[],
        Actor::User,
        add_node("decision", "Rust"),
    )
    .err()
    .unwrap();

    assert_eq!(err.to_string(), "decision \"Rust\" is already in the map");
}

#[test]
fn revise_allows_the_same_name_under_a_different_project_s_path() {
    let log = FakeLog::default();
    let here = Scope::Project(PathBuf::from("/here"));
    let there = Scope::Project(PathBuf::from("/there"));

    let first = revise(
        &log,
        "decisions",
        &here,
        &[],
        Actor::User,
        add_node("decision", "Rust"),
    )
    .unwrap();
    record_at(&log, "/here", first);

    let elsewhere = revise(
        &log,
        "decisions",
        &there,
        &[],
        Actor::User,
        add_node("decision", "Rust"),
    )
    .unwrap();
    record_at(&log, "/there", elsewhere);

    assert!(fold_map(&log, "decisions", &there)
        .unwrap()
        .find("decision", "Rust")
        .is_some());
}

#[test]
fn revising_the_code_map_is_refused() {
    let err = revise(
        &FakeLog::default(),
        "code",
        &scope(),
        &[],
        Actor::User,
        add_node("file", "src/main.rs"),
    )
    .err()
    .unwrap();

    assert!(err.to_string().starts_with("\"code\" is derived"), "{err}");
}

#[test]
fn an_unknown_map_is_an_error() {
    let err = fold_map(&FakeLog::default(), "glossary", &scope())
        .err()
        .unwrap();

    assert_eq!(
        err.to_string(),
        "no map named \"glossary\"; maps are decisions, tasks, code"
    );
}

#[test]
fn a_source_is_checked_against_the_loaded_log() {
    let cited = Event::message_received(Actor::User, "hi".to_string(), source("t"), None);
    let known = cited.id().as_uuid().to_string();
    let log = FakeLog::seeded(vec![cited]);
    let unknown = Uuid::now_v7().to_string();

    let ok = revise(
        &log,
        "decisions",
        &scope(),
        &[known],
        Actor::User,
        add_node("decision", "Rust"),
    )
    .unwrap();
    let missing = revise(
        &log,
        "decisions",
        &scope(),
        std::slice::from_ref(&unknown),
        Actor::User,
        add_node("decision", "Go"),
    )
    .err()
    .unwrap();
    let junk = revise(
        &log,
        "decisions",
        &scope(),
        &["user".to_string()],
        Actor::User,
        add_node("decision", "Go"),
    )
    .err()
    .unwrap();

    assert!(matches!(ok, Payload::NodeAdded { sources, .. } if sources.len() == 1));
    assert_eq!(missing.to_string(), format!("no event with id {unknown}"));
    assert_eq!(junk.to_string(), "\"user\" is not an event id");
}

#[test]
fn a_node_line_carries_its_id_sources_actor_and_time() {
    let map = Map::empty(&crate::core::DECISIONS);
    let node = Node {
        id: NodeId::new(),
        kind: "evidence".to_string(),
        name: "Built both".to_string(),
        properties: BTreeMap::from([("summary".to_string(), "side by side".to_string())]),
        sources: vec![EventId::new()],
        actor: Actor::User,
        added_at: Timestamp::now(),
    };

    let line: serde_json::Value = serde_json::from_str(&encode_node(&map, &node)).unwrap();

    assert_eq!(line["node"], node.id.as_uuid().to_string());
    assert_eq!(line["kind"], "evidence");
    assert_eq!(line["name"], "Built both");
    assert_eq!(line["properties"]["summary"], "side by side");
    assert_eq!(line["sources"][0], node.sources[0].as_uuid().to_string());
    assert_eq!(line["actor"], "user");
    assert_eq!(line["added_at"], node.added_at.to_string());
}

#[test]
fn a_derived_map_s_lines_carry_no_actor_or_time() {
    let mut map = Map::empty(&crate::core::CODE);
    map.apply(
        Mutation::AddNode {
            kind: "file".to_string(),
            name: "src/main.rs".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
        Actor::System,
    )
    .unwrap();

    let line: serde_json::Value =
        serde_json::from_str(&encode_node(&map, &map.nodes()[0])).unwrap();

    assert!(line.get("actor").is_none(), "{line}");
    assert!(line.get("added_at").is_none(), "{line}");
}

#[test]
fn an_edge_line_names_its_ends_as_kind_and_name() {
    let mut map = Map::empty(&crate::core::CODE);
    for (kind, name) in [("file", "src/main.rs"), ("package", "clap")] {
        map.apply(
            Mutation::AddNode {
                kind: kind.to_string(),
                name: name.to_string(),
                properties: BTreeMap::new(),
                sources: Vec::new(),
            },
            Actor::User,
        )
        .unwrap();
    }
    map.apply(
        Mutation::AddEdge {
            kind: "imports".to_string(),
            from: NodeRef {
                kind: "file".to_string(),
                name: "src/main.rs".to_string(),
            },
            to: NodeRef {
                kind: "package".to_string(),
                name: "clap".to_string(),
            },
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();

    let line: serde_json::Value =
        serde_json::from_str(&encode_edge(&map, &map.edges()[0])).unwrap();

    assert_eq!(line["edge"], "imports");
    assert_eq!(line["from"], "file:src/main.rs");
    assert_eq!(line["to"], "package:clap");
    assert_eq!(line["sources"], serde_json::json!([]));
}
