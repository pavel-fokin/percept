use super::*;
use crate::testing::{node_added, node_added_at, scope, FakeLog};

#[test]
fn spec_names_the_tool_and_carries_valid_schema_json() {
    let tool = ReadMap::new(Arc::new(FakeLog::default()), scope());
    let spec = tool.spec();
    assert_eq!(spec.name, "read_map");
    let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
    assert_eq!(schema["type"], "object");
}

#[test]
fn a_map_reads_as_its_nodes_and_edges() {
    let log = FakeLog::seeded(vec![node_added("decision", "JSONL for the log")]);
    let out = ReadMap::new(Arc::new(log), scope())
        .run(r#"{"map":"decisions"}"#)
        .unwrap();
    assert!(out.content.contains("decision"));
    assert!(out.content.contains("JSONL for the log"));
    assert!(out.commits.is_empty());
}

#[test]
fn an_empty_map_says_so() {
    let out = ReadMap::new(Arc::new(FakeLog::default()), scope())
        .run(r#"{"map":"decisions"}"#)
        .unwrap();
    assert!(out.content.contains("empty"));
}

#[test]
fn a_node_from_another_project_never_reaches_the_read() {
    let log = FakeLog::seeded(vec![node_added_at(
        "/other",
        "decision",
        "Not this project's",
    )]);
    let out = ReadMap::new(Arc::new(log), scope())
        .run(r#"{"map":"decisions"}"#)
        .unwrap();
    assert!(out.content.contains("empty"));
}

#[test]
fn an_unknown_map_is_an_error() {
    let tool = ReadMap::new(Arc::new(FakeLog::default()), scope());
    let Err(err) = tool.run(r#"{"map":"plans"}"#) else {
        panic!("expected an error")
    };
    assert!(err.to_string().contains("no map named"));
}

#[test]
fn a_missing_name_is_an_error() {
    let tool = ReadMap::new(Arc::new(FakeLog::default()), scope());
    assert!(tool.run("{}").is_err());
}

fn linked_log() -> (FakeLog, crate::percept::EventId) {
    use crate::percept::{Actor, Event, Map, Mutation, DECISIONS};
    use crate::testing::{node_ref, source};
    let evidence = Event::message_received(
        Actor::User,
        "Keep this rationale".into(),
        source("test"),
        None,
    );
    let id = evidence.id();
    let mut events = vec![evidence];
    let mut map = Map::empty(&DECISIONS);
    for (kind, name) in [
        ("commitment", "Storage"),
        ("decision", "JSONL"),
        ("question", "Where?"),
    ] {
        let payload = map
            .apply(Mutation::AddNode {
                kind: kind.into(),
                name: name.into(),
                properties: Default::default(),
                sources: vec![id],
            })
            .unwrap();
        events.push(Event::new(Actor::Model, source("test"), None, payload));
    }
    for (kind, from, to) in [
        (
            "details",
            node_ref("commitment", "Storage"),
            node_ref("decision", "JSONL"),
        ),
        (
            "resolves",
            node_ref("decision", "JSONL"),
            node_ref("question", "Where?"),
        ),
    ] {
        let payload = map
            .apply(Mutation::AddEdge {
                kind: kind.into(),
                from,
                to,
                sources: vec![id],
            })
            .unwrap();
        events.push(Event::new(Actor::Model, source("test"), None, payload));
    }
    (FakeLog::seeded(events), id)
}

fn read_rows(args: &str) -> Vec<serde_json::Value> {
    let (log, _) = linked_log();
    ReadMap::new(Arc::new(log), scope())
        .run(args)
        .unwrap()
        .content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn a_fragment_exposes_its_boundary_and_source_event_ids() {
    let (log, id) = linked_log();
    let out = ReadMap::new(Arc::new(log), scope())
        .run(r#"{"map":"decisions","around":{"kind":"commitment","name":"Storage"}}"#)
        .unwrap();
    let rows: Vec<serde_json::Value> = out
        .content
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows[0]["shown_nodes"], 2);
    assert_eq!(rows[0]["total_nodes"], 3);
    assert_eq!(rows[0]["boundary_edges"], 1);
    assert_eq!(rows[0]["incomplete"], true);
    for row in &rows[1..] {
        assert_eq!(row["sources"][0], id.as_uuid().to_string());
    }
    assert!(out.commits.is_empty());
}

#[test]
fn kind_filtering_happens_after_traversal() {
    let rows = read_rows(
        r#"{"map":"decisions","around":{"kind":"commitment","name":"Storage"},"depth":2,"kinds":["question"]}"#,
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["name"], "Where?");
    assert_eq!(rows[0]["boundary_edges"], 1);
}

#[test]
fn an_empty_selection_is_not_an_empty_recorded_map() {
    let rows = read_rows(r#"{"map":"decisions","kinds":["option"]}"#);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["shown_nodes"], 0);
    assert_eq!(rows[0]["total_nodes"], 3);
    assert_eq!(rows[0]["incomplete"], true);
}

#[test]
fn a_full_read_is_complete_only_as_a_recorded_map() {
    let rows = read_rows(r#"{"map":"decisions"}"#);
    assert_eq!(rows[0]["incomplete"], false);
    assert_eq!(rows[0]["shown_edges"], 2);
    assert!(rows[0]["notice"]
        .as_str()
        .unwrap()
        .contains("interpretations and relationships may still be incomplete"));
}

#[test]
fn invalid_selectors_are_refused_instead_of_ignored() {
    let (log, _) = linked_log();
    let tool = ReadMap::new(Arc::new(log), scope());
    for args in [
        r#"{"map":"decisions","depth":2}"#,
        r#"{"map":"decisions","around":{"kind":"commitment","name":"Missing"}}"#,
        r#"{"map":"decisions","kinds":["missing"]}"#,
        r#"{"map":"decisions","around":{"kind":"commitment","name":"Storage"},"depth":-1}"#,
    ] {
        assert!(tool.run(args).is_err(), "{args}");
    }
}
