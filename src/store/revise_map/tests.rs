use super::*;
use crate::percept::{Actor, Event, EventId};
use crate::testing::FakeLog;

fn tool(events: Vec<Event>) -> ReviseMap {
    let log = Arc::new(FakeLog::seeded(events));
    ReviseMap::new(log)
}

#[test]
fn spec_names_the_tool_and_carries_valid_schema_json() {
    let spec = tool(Vec::new()).spec();
    assert_eq!(spec.name, "revise_map");
    let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
    assert_eq!(schema["type"], "object");
}

#[test]
fn a_valid_batch_returns_the_payloads_and_content() {
    let cited = Event::message_received(
        Actor::User,
        "Go or Rust?".to_string(),
        "tui".to_string(),
        None,
    );
    let cited_id = cited.id();
    let revise = tool(vec![cited]);

    let args = format!(
        r#"{{"map":"decisions","changes":[{{"op":"add_node","kind":"option","name":"Rust","properties":{{"summary":"fast"}},"sources":["{}"]}}]}}"#,
        cited_id.as_uuid()
    );

    let output = revise.run(&args).unwrap();

    assert_eq!(output.commits.len(), 1);
    let node_id = match &output.commits[0] {
        Payload::NodeAdded {
            node,
            kind,
            name,
            properties,
            sources,
            ..
        } => {
            assert_eq!(kind, "option");
            assert_eq!(name, "Rust");
            assert_eq!(properties["summary"], "fast");
            assert_eq!(sources, &vec![cited_id]);
            *node
        }
        _ => panic!("expected a NodeAdded payload"),
    };
    assert_eq!(
        output.content,
        format!("added option \"Rust\" as {}", node_id.as_uuid())
    );
}

#[test]
fn a_failing_change_names_its_index_and_commits_nothing() {
    let cited = Event::message_received(Actor::User, "Rust".to_string(), "tui".to_string(), None);
    let id = cited.id().as_uuid().to_string();
    let revise = tool(vec![cited]);

    let err = revise
        .run(&format!(
            r#"{{"map":"decisions","changes":[
                {{"op":"add_node","kind":"option","name":"Rust","sources":["{id}"]}},
                {{"op":"add_node","kind":"goal","name":"Ship","sources":["{id}"]}}
            ]}}"#
        ))
        .err()
        .unwrap();

    assert!(err.to_string().starts_with("change 1: "), "{err}");
    assert_eq!(
        revise.log.load().unwrap().len(),
        1,
        "a refused batch must append nothing"
    );
}

#[test]
fn a_node_with_no_sources_is_refused_and_the_error_names_the_rule() {
    let revise = tool(Vec::new());

    let err = revise
        .run(r#"{"map":"decisions","changes":[{"op":"add_node","kind":"option","name":"Rust","sources":[]}]}"#)
        .err()
        .unwrap();

    assert!(err.to_string().contains("cites no sources"), "{err}");
    assert!(
        revise
            .run(
                r#"{"map":"decisions","changes":[{"op":"add_node","kind":"option","name":"Rust"}]}"#
            )
            .is_err(),
        "an omitted sources list is as empty as an empty one"
    );
}

#[test]
fn an_unknown_map_is_an_error() {
    let revise = tool(Vec::new());

    let err = revise
        .run(r#"{"map":"tasks","changes":[{"op":"add_node","kind":"goal","name":"Ship","sources":[]}]}"#)
        .err()
        .unwrap();

    assert!(err.to_string().contains("decisions"), "{err}");
}

#[test]
fn the_code_map_is_refused_the_same_as_the_cli() {
    let revise = tool(Vec::new());

    let err = revise
        .run(r#"{"map":"code","changes":[{"op":"add_node","kind":"file","name":"src/main.rs","sources":[]}]}"#)
        .err()
        .unwrap();

    assert!(
        err.to_string().contains("derived from the working tree"),
        "{err}"
    );
}

#[test]
fn an_empty_changes_list_is_an_error() {
    let revise = tool(Vec::new());

    assert!(revise.run(r#"{"map":"decisions","changes":[]}"#).is_err());
}

#[test]
fn a_sources_id_the_log_lacks_is_an_error() {
    let revise = tool(Vec::new());
    let unknown = EventId::new().as_uuid().to_string();

    let err = revise
        .run(&format!(
            r#"{{"map":"decisions","changes":[{{"op":"add_node","kind":"option","name":"Rust","sources":["{unknown}"]}}]}}"#
        ))
        .err()
        .unwrap();

    assert!(err.to_string().contains("no event with id"), "{err}");
}

#[test]
fn a_change_can_reference_a_node_an_earlier_change_just_added() {
    let cited = Event::message_received(Actor::User, "Rust".to_string(), "tui".to_string(), None);
    let id = cited.id().as_uuid().to_string();
    let revise = tool(vec![cited]);

    let output = revise
        .run(&format!(
            r#"{{"map":"decisions","changes":[
                {{"op":"add_node","kind":"question","name":"Which language?","sources":["{id}"]}},
                {{"op":"add_node","kind":"decision","name":"Rust over Go","sources":["{id}"]}},
                {{"op":"add_edge","kind":"resolves","from":{{"kind":"decision","name":"Rust over Go"}},"to":{{"kind":"question","name":"Which language?"}},"sources":[]}}
            ]}}"#
        ))
        .unwrap();

    assert_eq!(output.commits.len(), 3);
    assert!(matches!(output.commits[2], Payload::EdgeAdded { .. }));
    assert_eq!(
        output.content.lines().last().unwrap(),
        "added edge decision \"Rust over Go\" resolves question \"Which language?\""
    );
}
