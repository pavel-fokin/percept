use super::*;
use crate::core::testing::{edge_added, node_added, node_added_at, schemas, scope, FakeLog};
use crate::core::Event;
use crate::mapstore::LogMaps;

/// A `read_map` over the log-folded maps, the way `main` wires it for
/// every map but `code`.
fn tool(log: FakeLog) -> ReadMap {
    ReadMap::new(Arc::new(LogMaps::new(
        Arc::new(log),
        Arc::new(schemas()),
        scope(),
    )))
}

#[test]
fn spec_names_the_tool_and_carries_valid_schema_json() {
    let spec = tool(FakeLog::default()).spec();
    assert_eq!(spec.name, "read_map");
    let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
    assert_eq!(schema["type"], "object");
}

#[test]
fn a_map_reads_as_its_nodes_and_edges() {
    let log = FakeLog::seeded(vec![node_added("decision", "JSONL for the log")]);
    let out = tool(log).run(r#"{"map":"decisions"}"#).unwrap();
    assert!(out.content.contains("decision"));
    assert!(out.content.contains("JSONL for the log"));
    assert!(out.commits.is_empty());
}

#[test]
fn an_empty_map_says_so() {
    let out = tool(FakeLog::default())
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
    let out = tool(log).run(r#"{"map":"decisions"}"#).unwrap();
    assert!(out.content.contains("nothing has been recorded"));
}

#[test]
fn an_unknown_map_is_an_error() {
    let Err(err) = tool(FakeLog::default()).run(r#"{"map":"plans"}"#) else {
        panic!("expected an error")
    };
    assert!(err.to_string().contains("no map named"));
}

#[test]
fn a_missing_name_is_an_error() {
    assert!(tool(FakeLog::default()).run("{}").is_err());
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
    let out = tool(FakeLog::seeded(weighed_question())).run(args)?;
    Ok(out
        .content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect())
}

#[test]
fn a_read_opens_with_the_schema_then_the_counts_then_the_nodes_then_the_edges() {
    let rows = read(r#"{"map":"decisions","around":{"kind":"question","name":"Where?"}}"#).unwrap();

    assert_eq!(rows[0]["schema"], "decisions");
    assert_eq!(rows[1]["shown_nodes"], 3);
    assert_eq!(rows[1]["total_nodes"], 4);
    assert_eq!(rows[1]["boundary_edges"], 1);
    assert!(rows[1].get("note").is_none());
    assert_eq!(rows.len(), 1 + 1 + 3 + 2);
    assert!(rows[2..=4].iter().all(|row| row["sources"].is_array()));
    assert!(rows[5..].iter().all(|row| row["edge"].is_string()));
}

#[test]
fn the_schema_line_glosses_every_kind_so_a_selector_is_not_a_guess() {
    let rows = read(r#"{"map":"decisions"}"#).unwrap();

    let option = rows[0]["node_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kind| kind["name"] == "option")
        .unwrap();
    assert!(option["gloss"]
        .as_str()
        .unwrap()
        .contains("weighed and lost"));
    let resolves = rows[0]["edge_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kind| kind["name"] == "resolves")
        .unwrap();
    assert!(resolves["gloss"]
        .as_str()
        .unwrap()
        .contains("the question it settles"));
}

#[test]
fn an_empty_cut_of_a_full_map_is_not_an_empty_map() {
    let rows = read(r#"{"map":"decisions","since":"2999-01-01T00:00:00Z"}"#).unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["shown_nodes"], 0);
    assert_eq!(rows[1]["total_nodes"], 4);
    assert!(rows[1].get("note").is_none());
}

#[test]
fn depth_without_around_is_a_whole_read_not_a_wasted_call() {
    let rows = read(r#"{"map":"decisions","depth":2}"#).unwrap();

    assert_eq!(rows[1]["shown_nodes"], 4);
}

#[test]
fn around_on_an_empty_map_still_says_nothing_is_recorded() {
    let out = tool(FakeLog::default())
        .run(r#"{"map":"decisions","around":{"kind":"question","name":"Where?"}}"#)
        .unwrap();

    assert!(out.content.contains("nothing has been recorded"));
}

#[test]
fn since_takes_the_shorthand_the_cli_takes() {
    let rows = read(r#"{"map":"decisions","since":"1d"}"#).unwrap();

    assert_eq!(rows[1]["shown_nodes"], 4, "everything was added just now");
}

#[test]
fn a_since_that_is_neither_iso8601_nor_shorthand_is_an_error() {
    assert!(read(r#"{"map":"decisions","since":"yesterday"}"#).is_err());
}
