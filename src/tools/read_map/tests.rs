use std::path::PathBuf;

use super::*;
use crate::core::testing::{
    edge_added, human, node_added, node_added_at, node_added_citing, node_id, schemas, source,
    FakeLog, ROOT,
};
use crate::core::{Actor, Event, Payload};
use crate::mapstore::LogMaps;

/// A `read_map` over the log-folded maps, the way `main` wires it.
fn tool(log: FakeLog) -> ReadMap {
    let log = Arc::new(log);
    ReadMap::new(
        Arc::new(LogMaps::new(
            log.clone(),
            Arc::new(schemas()),
            PathBuf::from(ROOT),
        )),
        log,
    )
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
    let log = FakeLog::seeded(vec![node_added("verdict", "JSONL for the log")]);
    let out = tool(log).run(r#"{"map":"debates"}"#).unwrap();
    assert!(out.content.contains("verdict"));
    assert!(out.content.contains("JSONL for the log"));
    assert!(out.commits.is_empty());
}

#[test]
fn a_selected_node_previews_what_each_source_event_said() {
    let instruction = Event::message_received(
        Actor::Human(human()),
        "record this".to_string(),
        source("test"),
        None,
    );
    let evidence = Event::message_received(
        Actor::Agent,
        "generation belongs to the server because the browser can disconnect ".repeat(4),
        source("test"),
        None,
    );
    let node = node_added_citing(
        Actor::Agent,
        "verdict",
        "server-owned generation",
        vec![instruction.id(), evidence.id()],
    );
    let out = tool(FakeLog::seeded(vec![instruction, evidence, node]))
        .run(r#"{"map":"debates","around":"v1"}"#)
        .unwrap();
    let row: serde_json::Value = serde_json::from_str(out.content.lines().nth(2).unwrap()).unwrap();

    assert_eq!(row["sources"].as_array().unwrap().len(), 2);
    assert_eq!(row["source_previews"][0]["actor"]["kind"], "human");
    assert_eq!(row["source_previews"][0]["type"], "message.received");
    assert_eq!(row["source_previews"][0]["payload"]["content"], "record this");
    assert_eq!(row["source_previews"][1]["actor"]["kind"], "agent");
    assert!(
        row["source_previews"][1]["preview"]["len"]
            .as_u64()
            .unwrap()
            > crate::store::PREVIEW_CHARS as u64
    );
}

#[test]
fn a_whole_map_keeps_only_source_ids() {
    let evidence = Event::message_received(
        Actor::Human(human()),
        "the reason".to_string(),
        source("test"),
        None,
    );
    let node = node_added_citing(
        Actor::Human(human()),
        "verdict",
        "keep the overview compact",
        vec![evidence.id()],
    );
    let out = tool(FakeLog::seeded(vec![evidence, node]))
        .run(r#"{"map":"debates"}"#)
        .unwrap();
    let row: serde_json::Value = serde_json::from_str(out.content.lines().nth(2).unwrap()).unwrap();

    assert_eq!(row["sources"].as_array().unwrap().len(), 1);
    assert!(row.get("source_previews").is_none());
}

#[test]
fn a_selected_edge_previews_its_source_event() {
    let topic = node_added("topic", "Who owns generation?");
    let verdict = node_added("verdict", "the server");
    let evidence = Event::message_received(
        Actor::Human(human()),
        "closing the tab must not cancel generation".to_string(),
        source("test"),
        None,
    );
    let edge = Event::new(
        Actor::Agent,
        source("test"),
        None,
        Payload::EdgeAdded {
            map: "debates".to_string(),
            kind: "settles".to_string(),
            from: node_id(&verdict),
            to: node_id(&topic),
            sources: vec![evidence.id()],
        },
    );
    let out = tool(FakeLog::seeded(vec![topic, verdict, evidence, edge]))
        .run(r#"{"map":"debates","around":"t1"}"#)
        .unwrap();
    let row: serde_json::Value = out
        .content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|row: &serde_json::Value| row["edge"] == "settles")
        .unwrap();

    assert_eq!(row["sources"].as_array().unwrap().len(), 1);
    assert_eq!(
        row["source_previews"][0]["payload"]["content"],
        "closing the tab must not cancel generation"
    );
}

#[test]
fn an_empty_map_says_so() {
    let out = tool(FakeLog::default())
        .run(r#"{"map":"debates"}"#)
        .unwrap();
    assert!(out.content.contains("nothing has been recorded"));
}

#[test]
fn a_node_from_another_project_never_reaches_the_read() {
    let log = FakeLog::seeded(vec![node_added_at(
        "/other",
        "verdict",
        "Not this project's",
    )]);
    let out = tool(log).run(r#"{"map":"debates"}"#).unwrap();
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

/// A topic with its verdict, one claim, and the fact for that
/// claim: four nodes, three edges, every edge one hop from the last.
fn weighed_topic() -> Vec<Event> {
    let topic = node_added("topic", "Where?");
    let verdict = node_added("verdict", "JSONL");
    let claim = node_added("claim", "SQLite");
    let fact = node_added("fact", "benchmarks");
    let settles = edge_added("settles", &verdict, &topic);
    let about = edge_added("about", &claim, &topic);
    let backs = edge_added("backs", &fact, &claim);
    vec![
        topic, verdict, claim, fact, settles, about, backs,
    ]
}

fn read(args: &str) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let out = tool(FakeLog::seeded(weighed_topic())).run(args)?;
    Ok(out
        .content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect())
}

#[test]
fn a_read_opens_with_the_schema_then_the_counts_then_the_nodes_then_the_edges() {
    let rows = read(r#"{"map":"debates","around":{"kind":"topic","name":"Where?"}}"#).unwrap();

    assert_eq!(rows[0]["schema"], "debates");
    assert_eq!(rows[1]["shown_nodes"], 3);
    assert_eq!(rows[1]["total_nodes"], 4);
    assert_eq!(rows[1]["boundary_edges"], 1);
    assert!(rows[1].get("note").is_none());
    assert_eq!(rows.len(), 1 + 1 + 3 + 2);
    assert!(rows[2..=4].iter().all(|row| row["sources"].is_array()));
    assert!(rows[5..].iter().all(|row| row["edge"].is_string()));
}

#[test]
fn the_schema_line_glosses_a_kind_so_a_selector_is_not_a_guess() {
    let rows = read(r#"{"map":"debates"}"#).unwrap();

    let claim = rows[0]["node_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kind| kind["name"] == "claim")
        .unwrap();
    assert!(claim["gloss"]
        .as_str()
        .unwrap()
        .contains("saying why"));
    let doubts = rows[0]["edge_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kind| kind["name"] == "doubts")
        .unwrap();
    assert!(doubts["gloss"]
        .as_str()
        .unwrap()
        .contains("the verdict stands until"));
}

#[test]
fn an_empty_cut_of_a_full_map_is_not_an_empty_map() {
    let rows = read(r#"{"map":"debates","since":"2999-01-01T00:00:00Z"}"#).unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["shown_nodes"], 0);
    assert_eq!(rows[1]["total_nodes"], 4);
    assert!(rows[1].get("note").is_none());
}

#[test]
fn depth_without_around_is_a_whole_read_not_a_wasted_call() {
    let rows = read(r#"{"map":"debates","depth":2}"#).unwrap();

    assert_eq!(rows[1]["shown_nodes"], 4);
}

#[test]
fn around_on_an_empty_map_still_says_nothing_is_recorded() {
    let out = tool(FakeLog::default())
        .run(r#"{"map":"debates","around":{"kind":"topic","name":"Where?"}}"#)
        .unwrap();

    assert!(out.content.contains("nothing has been recorded"));
}

#[test]
fn since_takes_the_shorthand_the_cli_takes() {
    let rows = read(r#"{"map":"debates","since":"1d"}"#).unwrap();

    assert_eq!(rows[1]["shown_nodes"], 4, "everything was added just now");
}

#[test]
fn a_since_that_is_neither_iso8601_nor_shorthand_is_an_error() {
    assert!(read(r#"{"map":"debates","since":"yesterday"}"#).is_err());
}
