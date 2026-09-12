use super::*;
use crate::core::testing::{human, schemas, source, source_at, FakeLog};
use crate::core::{Actor, Change, Event, NodeId, NodeRef};
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

#[test]
fn a_claim_without_a_why_is_refused_as_a_new_write() {
    let log = FakeLog::default();
    let source = source("cli");

    let err = commit(
        &log,
        &schemas(),
        "debates",
        &source,
        &[],
        Actor::Human(human()),
        add_node("claim", "SQLite"),
    )
    .err()
    .unwrap()
    .to_string();

    assert!(err.contains("lacks its `why` property"), "{err}");
    assert!(err.contains("saying why"), "{err}");
}

#[test]
fn a_chore_without_a_why_is_refused_as_a_new_write() {
    let log = FakeLog::default();
    let source = source("cli");

    let err = commit(
        &log,
        &schemas(),
        "chores",
        &source,
        &[],
        Actor::Human(human()),
        add_node("chore", "cancel a turn without quitting"),
    )
    .err()
    .unwrap()
    .to_string();

    assert!(err.contains("lacks its `why` property"), "{err}");
    assert!(err.contains("why it matters"), "{err}");
}

#[test]
fn a_claim_with_a_why_is_recorded() {
    let log = FakeLog::default();
    let source = source("cli");
    let mutation = |sources| Mutation::AddNode {
        kind: "claim".to_string(),
        name: "SQLite".to_string(),
        properties: BTreeMap::from([("why".to_string(), "one more dependency".to_string())]),
        sources,
    };

    let event = commit(
        &log,
        &schemas(),
        "debates",
        &source,
        &[],
        Actor::Human(human()),
        mutation,
    )
    .unwrap();

    assert!(matches!(event.payload(), Payload::NodeAdded { .. }));
}

#[test]
fn commit_appends_the_event_that_records_the_mutation() {
    let log = FakeLog::default();
    let source = source("cli");

    let event = commit(
        &log,
        &schemas(),
        "debates",
        &source,
        &[],
        Actor::Human(human()),
        add_node("verdict", "Rust"),
    )
    .unwrap();

    assert!(matches!(event.payload(), Payload::NodeAdded { name, .. } if name == "Rust"));
    assert!(fold_map(&log, &schemas(), "debates", &source.path)
        .unwrap()
        .find("verdict", "Rust")
        .is_some());
}

#[test]
fn commit_loads_the_log_so_a_second_call_sees_the_first() {
    let log = FakeLog::default();
    let source = source("cli");
    commit(
        &log,
        &schemas(),
        "debates",
        &source,
        &[],
        Actor::Human(human()),
        add_node("verdict", "Rust"),
    )
    .unwrap();

    let err = commit(
        &log,
        &schemas(),
        "debates",
        &source,
        &[],
        Actor::Human(human()),
        add_node("verdict", "Rust"),
    )
    .err()
    .unwrap();

    assert_eq!(err.to_string(), "verdict \"Rust\" is already in the map");
}

#[test]
fn commit_allows_the_same_name_under_a_different_path() {
    let log = FakeLog::default();
    let here = source_at("cli", "/here");
    let there = source_at("cli", "/there");

    commit(
        &log,
        &schemas(),
        "debates",
        &here,
        &[],
        Actor::Human(human()),
        add_node("verdict", "Rust"),
    )
    .unwrap();
    commit(
        &log,
        &schemas(),
        "debates",
        &there,
        &[],
        Actor::Human(human()),
        add_node("verdict", "Rust"),
    )
    .unwrap();

    assert!(fold_map(&log, &schemas(), "debates", &there.path)
        .unwrap()
        .find("verdict", "Rust")
        .is_some());
}

#[test]
fn of_path_keeps_only_the_events_whose_source_ran_there() {
    let here = Event::message_received(
        Actor::Human(human()),
        "hi".to_string(),
        source_at("cli", "/here"),
        None,
    );
    let there = Event::message_received(
        Actor::Human(human()),
        "hi".to_string(),
        source_at("cli", "/there"),
        None,
    );
    let events = vec![here.clone(), there];

    let kept: Vec<_> = of_path(&events, &here.source().path).collect();

    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].id(), here.id());
}

#[test]
fn paths_lists_the_distinct_source_paths_sorted() {
    let events = vec![
        Event::message_received(Actor::Human(human()), "a".to_string(), source_at("cli", "/there"), None),
        Event::message_received(Actor::Human(human()), "b".to_string(), source_at("cli", "/here"), None),
        Event::message_received(Actor::Human(human()), "c".to_string(), source_at("cli", "/there"), None),
    ];

    assert_eq!(
        paths(&events),
        vec![PathBuf::from("/here"), PathBuf::from("/there")]
    );
}

#[test]
fn committing_to_a_map_no_schema_declares_is_an_error() {
    let source = source("cli");
    let err = commit(
        &FakeLog::default(),
        &schemas(),
        "code",
        &source,
        &[],
        Actor::Human(human()),
        add_node("file", "src/main.rs"),
    )
    .err()
    .unwrap();

    assert!(err.to_string().starts_with("no map named \"code\""), "{err}");
}

#[test]
fn an_unknown_map_is_an_error() {
    let source = source("cli");
    let err = fold_map(&FakeLog::default(), &schemas(), "glossary", &source.path)
        .err()
        .unwrap();

    assert_eq!(
        err.to_string(),
        "no map named \"glossary\"; maps are debates, chores"
    );
}

#[test]
fn a_source_is_checked_against_the_loaded_log() {
    let source = source("cli");
    let cited = Event::message_received(Actor::Human(human()), "hi".to_string(), source.clone(), None);
    let known = cited.id().as_uuid().to_string();
    let log = FakeLog::seeded(vec![cited]);
    let unknown = Uuid::now_v7().to_string();

    let ok = commit(
        &log,
        &schemas(),
        "debates",
        &source,
        &[known],
        Actor::Human(human()),
        add_node("verdict", "Rust"),
    )
    .unwrap();
    let missing = commit(
        &log,
        &schemas(),
        "debates",
        &source,
        std::slice::from_ref(&unknown),
        Actor::Human(human()),
        add_node("verdict", "Go"),
    )
    .err()
    .unwrap();
    let junk = commit(
        &log,
        &schemas(),
        "debates",
        &source,
        &["user".to_string()],
        Actor::Human(human()),
        add_node("verdict", "Go"),
    )
    .err()
    .unwrap();

    assert!(matches!(ok.payload(), Payload::NodeAdded { sources, .. } if sources.len() == 1));
    assert_eq!(missing.to_string(), format!("no event with id {unknown}"));
    assert_eq!(junk.to_string(), "\"user\" is not an event id");
}

#[test]
fn a_node_line_carries_its_id_sources_actor_and_time() {
    let map = Map::empty(crate::core::testing::debates());
    let me = crate::core::testing::human();
    let node = Node {
        id: NodeId::new(),
        kind: "fact".to_string(),
        name: "Built both".to_string(),
        properties: BTreeMap::from([("summary".to_string(), "side by side".to_string())]),
        sources: vec![EventId::new()],
        history: vec![
            Change {
                actor: Actor::Human(me),
                at: Timestamp::now(),
                why: None,
            },
            Change {
                actor: Actor::Agent,
                at: Timestamp::now(),
                why: Some("looked stale".to_string()),
            },
        ],
        seq: 1,
    };

    let line: serde_json::Value = serde_json::from_str(&encode_node(&map, &node, true)).unwrap();

    assert_eq!(line["node"], node.id.as_uuid().to_string());
    assert_eq!(line["kind"], "fact");
    assert_eq!(line["name"], "Built both");
    assert_eq!(line["properties"]["summary"], "side by side");
    assert_eq!(line["sources"][0], node.sources[0].as_uuid().to_string());
    assert_eq!(line["actor"]["kind"], "human");
    assert_eq!(line["actor"]["id"], me.unwrap().as_uuid().to_string());
    assert_eq!(line["added_at"], node.added().at.to_string());
    assert_eq!(line["changed_by"], "agent");
    assert_eq!(line["changed_why"], "looked stale");
}

#[test]
fn a_node_line_carries_its_short_id() {
    let mut map = Map::empty(crate::core::testing::debates());
    map.apply(
        Mutation::AddNode {
            kind: "fact".to_string(),
            name: "Built both".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();

    let line: serde_json::Value =
        serde_json::from_str(&encode_node(&map, &map.nodes()[0], true)).unwrap();

    assert_eq!(line["id"], "f1");
}

#[test]
fn an_edge_line_names_its_ends_as_kind_and_name() {
    let mut map = Map::empty(crate::core::testing::files());
    for (kind, name) in [("file", "src/main.rs"), ("package", "clap")] {
        map.apply(
            Mutation::AddNode {
                kind: kind.to_string(),
                name: name.to_string(),
                properties: BTreeMap::new(),
                sources: Vec::new(),
            },
            Actor::Human(human()),
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
        Actor::Human(human()),
    )
    .unwrap();

    let line: serde_json::Value =
        serde_json::from_str(&encode_edge(&map, &map.edges()[0], true)).unwrap();

    assert_eq!(line["edge"], "imports");
    assert_eq!(line["from"], "file:src/main.rs");
    assert_eq!(line["to"], "package:clap");
    assert_eq!(line["sources"], serde_json::json!([]));
}

fn map_with_a_verdict() -> Map {
    let mut map = Map::empty(crate::core::testing::debates());
    map.apply(
        Mutation::AddNode {
            kind: "verdict".to_string(),
            name: "Rust over Go".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
    map
}

#[test]
fn node_ref_args_resolves_a_kind_and_name_object() {
    let map = map_with_a_verdict();
    let args: NodeRefArgs =
        serde_json::from_str(r#"{"kind":"verdict","name":"Rust over Go"}"#).unwrap();

    let node = args.resolve(&map).unwrap();

    assert_eq!(map.node(node).unwrap().name, "Rust over Go");
}

#[test]
fn node_ref_args_resolves_a_bare_short_id_string() {
    let map = map_with_a_verdict();
    let args: NodeRefArgs = serde_json::from_str(r#""v1""#).unwrap();

    let node = args.resolve(&map).unwrap();

    assert_eq!(map.node(node).unwrap().name, "Rust over Go");
}

#[test]
fn node_ref_args_resolves_a_bare_kind_colon_name_string_too() {
    let map = map_with_a_verdict();
    let args: NodeRefArgs = serde_json::from_str(r#""verdict:Rust over Go""#).unwrap();

    let node = args.resolve(&map).unwrap();

    assert_eq!(map.node(node).unwrap().name, "Rust over Go");
}

#[test]
fn node_ref_args_object_form_rejects_a_field_neither_shape_has() {
    let result: Result<NodeRefArgs, _> = serde_json::from_str(r#"{"kind":"verdict"}"#);

    assert!(result.is_err(), "name is required and cannot default");
}
