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
