use super::*;
use crate::percept::Event;
use crate::testing::{edge_added, node_added, node_added_at, scope, FakeLog};

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
    assert!(out.content.contains("nothing has been recorded"));
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
    assert!(out.content.contains("nothing has been recorded"));
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

/// A question with its decision, one option, and the evidence for that
/// option: four nodes, three edges, every edge one hop from the last.
fn weighed_question() -> Vec<Event> {
    let question = node_added("question", "Where?");
    let decision = node_added("decision", "JSONL");
    let option = node_added("option", "SQLite");
    let evidence = node_added("evidence", "benchmarks");
    let resolves = edge_added("resolves", &decision, &question);
    let answers = edge_added("answers", &option, &question);
    let supports = edge_added("supports", &evidence, &option);
    vec![
        question, decision, option, evidence, resolves, answers, supports,
    ]
}

fn read(args: &str) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let out = ReadMap::new(Arc::new(FakeLog::seeded(weighed_question())), scope()).run(args)?;
    Ok(out
        .content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect())
}

#[test]
fn a_read_opens_with_the_counts_then_the_nodes_then_the_edges() {
    let rows = read(r#"{"map":"decisions","around":{"kind":"question","name":"Where?"}}"#).unwrap();

    assert_eq!(rows[0]["shown_nodes"], 3);
    assert_eq!(rows[0]["total_nodes"], 4);
    assert_eq!(rows[0]["boundary_edges"], 1);
    assert!(rows[0].get("note").is_none());
    assert_eq!(rows.len(), 1 + 3 + 2);
    assert!(rows[1..=3].iter().all(|row| row["sources"].is_array()));
    assert!(rows[4..].iter().all(|row| row["edge"].is_string()));
}

#[test]
fn an_empty_cut_of_a_full_map_is_not_an_empty_map() {
    let rows = read(r#"{"map":"decisions","since":"2999-01-01T00:00:00Z"}"#).unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["shown_nodes"], 0);
    assert_eq!(rows[0]["total_nodes"], 4);
    assert!(rows[0].get("note").is_none());
}

#[test]
fn depth_needs_around() {
    assert!(read(r#"{"map":"decisions","depth":2}"#).is_err());
}

#[test]
fn a_since_that_is_not_iso8601_is_an_error() {
    assert!(read(r#"{"map":"decisions","since":"yesterday"}"#).is_err());
}
