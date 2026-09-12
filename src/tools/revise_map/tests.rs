use std::path::PathBuf;

use super::*;
use crate::core::testing::{
    edge_added, human, node_added, node_added_by, node_id, schemas, source, FakeLog, ROOT,
};
use crate::core::{Actor, Event, EventId};

fn tool(events: Vec<Event>) -> ReviseMap {
    let log = Arc::new(FakeLog::seeded(events));
    ReviseMap::new(log, Arc::new(schemas()), PathBuf::from(ROOT))
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
    let cited =
        Event::message_received(Actor::Human(human()), "Go or Rust?".to_string(), source("tui"), None);
    let cited_id = cited.id();
    let revise = tool(vec![cited]);

    let args = format!(
        r#"{{"map":"debates","changes":[{{"op":"add_node","kind":"claim","name":"Rust","properties":{{"summary":"fast","why":"lost to Go on ecosystem"}},"sources":["{}"]}}]}}"#,
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
            assert_eq!(kind, "claim");
            assert_eq!(name, "Rust");
            assert_eq!(properties["summary"], "fast");
            assert_eq!(sources, &vec![cited_id]);
            *node
        }
        _ => panic!("expected a NodeAdded payload"),
    };
    assert_eq!(
        output.content,
        format!("added claim \"Rust\" as {}", node_id.as_uuid())
    );
}

#[test]
fn a_change_node_op_applies() {
    let added = node_added_by(Actor::Agent, "claim", "Rust");
    let id = node_id(&added);
    let source = added.id();
    let revise = tool(vec![added]);

    let args = format!(
        r#"{{"map":"debates","changes":[{{"op":"change_node","node":{{"kind":"claim","name":"Rust"}},"properties":{{"summary":"fast"}},"sources":["{}"]}}]}}"#,
        source.as_uuid()
    );

    let output = revise.run(&args).unwrap();

    assert_eq!(output.commits.len(), 1);
    match &output.commits[0] {
        Payload::NodeChanged {
            node,
            name,
            properties,
            ..
        } => {
            assert_eq!(*node, id);
            assert!(name.is_none());
            assert_eq!(properties["summary"], "fast");
        }
        _ => panic!("expected a NodeChanged payload"),
    }
    assert_eq!(output.content, "changed claim \"Rust\"");
}

#[test]
fn a_failing_change_names_its_index_and_commits_nothing() {
    let cited = Event::message_received(Actor::Human(human()), "Rust".to_string(), source("tui"), None);
    let id = cited.id().as_uuid().to_string();
    let revise = tool(vec![cited]);

    let err = revise
        .run(&format!(
            r#"{{"map":"debates","changes":[
                {{"op":"add_node","kind":"claim","name":"Rust","properties":{{"why":"lost to Go on ecosystem"}},"sources":["{id}"]}},
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
        .run(r#"{"map":"debates","changes":[{"op":"add_node","kind":"claim","name":"Rust","sources":[]}]}"#)
        .err()
        .unwrap();

    assert!(err.to_string().contains("cites no sources"), "{err}");
    assert!(
        revise
            .run(
                r#"{"map":"debates","changes":[{"op":"add_node","kind":"claim","name":"Rust"}]}"#
            )
            .is_err(),
        "an omitted sources list is as empty as an empty one"
    );
}

#[test]
fn a_claim_with_no_why_is_refused() {
    let cited = Event::message_received(Actor::Human(human()), "Rust".to_string(), source("tui"), None);
    let cited_id = cited.id();
    let revise = tool(vec![cited]);

    let args = format!(
        r#"{{"map":"debates","changes":[{{"op":"add_node","kind":"claim","name":"Rust","sources":["{}"]}}]}}"#,
        cited_id.as_uuid()
    );

    let err = revise.run(&args).err().unwrap();

    assert!(err.to_string().contains("lacks its `why` property"), "{err}");
}

#[test]
fn an_unknown_map_is_an_error() {
    let revise = tool(Vec::new());

    let err = revise
        .run(r#"{"map":"glossary","changes":[{"op":"add_node","kind":"goal","name":"Ship","sources":[]}]}"#)
        .err()
        .unwrap();

    assert!(err.to_string().contains("debates"), "{err}");
}

#[test]
fn a_map_named_code_fails_the_same_as_any_unknown_map() {
    let revise = tool(Vec::new());

    let err = revise
        .run(r#"{"map":"code","changes":[{"op":"add_node","kind":"file","name":"src/main.rs","sources":[]}]}"#)
        .err()
        .unwrap();

    assert!(err.to_string().contains("no map named \"code\""), "{err}");
}

#[test]
fn an_empty_changes_list_is_an_error() {
    let revise = tool(Vec::new());

    assert!(revise.run(r#"{"map":"debates","changes":[]}"#).is_err());
}

#[test]
fn a_sources_id_the_log_lacks_is_an_error() {
    let revise = tool(Vec::new());
    let unknown = EventId::new().as_uuid().to_string();

    let err = revise
        .run(&format!(
            r#"{{"map":"debates","changes":[{{"op":"add_node","kind":"claim","name":"Rust","sources":["{unknown}"]}}]}}"#
        ))
        .err()
        .unwrap();

    assert!(err.to_string().contains("no event with id"), "{err}");
}

#[test]
fn a_change_can_reference_a_node_an_earlier_change_just_added() {
    let cited = Event::message_received(Actor::Human(human()), "Rust".to_string(), source("tui"), None);
    let id = cited.id().as_uuid().to_string();
    let revise = tool(vec![cited]);

    let output = revise
        .run(&format!(
            r#"{{"map":"debates","changes":[
                {{"op":"add_node","kind":"topic","name":"Which language?","sources":["{id}"]}},
                {{"op":"add_node","kind":"verdict","name":"Rust over Go","sources":["{id}"]}},
                {{"op":"add_edge","kind":"settles","from":{{"kind":"verdict","name":"Rust over Go"}},"to":{{"kind":"topic","name":"Which language?"}},"sources":[]}}
            ]}}"#
        ))
        .unwrap();

    assert_eq!(output.commits.len(), 3);
    assert!(matches!(output.commits[2], Payload::EdgeAdded { .. }));
    assert_eq!(
        output.content.lines().last().unwrap(),
        "added edge verdict \"Rust over Go\" settles topic \"Which language?\""
    );
}

#[test]
fn removing_a_user_written_node_is_refused_by_the_map_s_own_rank_rule() {
    // No app-level guard names a kind here: the `NotYours` error the
    // model sees is `Map::apply`'s W6, the same rule that would refuse
    // a rename or a property change.
    let revise = tool(vec![node_added("topic", "Which language?")]);

    let err = revise
        .run(r#"{"map":"debates","changes":[{"op":"remove_node","node":{"kind":"topic","name":"Which language?"},"why":"wrong"}]}"#)
        .err()
        .unwrap()
        .to_string();

    assert!(err.contains("was written by human"), "{err}");
}

#[test]
fn removing_a_user_written_edge_is_refused() {
    let verdict = node_added("verdict", "Rust");
    let topic = node_added("topic", "Which language?");
    let edge = edge_added("settles", &verdict, &topic);
    let revise = tool(vec![verdict, topic, edge]);

    let err = revise
        .run(r#"{"map":"debates","changes":[{"op":"remove_edge","kind":"settles","from":{"kind":"verdict","name":"Rust"},"to":{"kind":"topic","name":"Which language?"},"why":"wrong"}]}"#)
        .err()
        .unwrap()
        .to_string();

    assert!(err.contains("was written by human"), "{err}");
}

#[test]
fn removing_a_model_node_that_a_user_edge_touches_is_refused() {
    // Removing a node drops every edge on it, so the map weighs each
    // edge as its own removal would be: the user's edge is not the
    // model's to drop.
    let model_node = node_added_by(Actor::Agent, "claim", "Rust");
    let topic = node_added("topic", "Which language?");
    let user_edge = edge_added("about", &model_node, &topic);
    let revise = tool(vec![model_node, topic, user_edge]);

    let err = match revise
        .run(r#"{"map":"debates","changes":[{"op":"remove_node","node":{"kind":"claim","name":"Rust"},"why":"wrong"}]}"#)
    {
        Ok(_) => panic!("the removal went through"),
        Err(err) => err.to_string(),
    };

    assert!(err.contains("touched by human"), "{err}");
}

#[test]
fn removing_a_model_written_node_is_allowed() {
    let revise = tool(vec![node_added_by(Actor::Agent, "claim", "Go")]);

    let output = revise
        .run(r#"{"map":"debates","changes":[{"op":"remove_node","node":{"kind":"claim","name":"Go"},"why":"wrong"}]}"#)
        .unwrap();

    assert!(matches!(output.commits[0], Payload::NodeRemoved { .. }));
}

#[test]
fn removing_a_model_written_verdict_is_allowed() {
    // The core enforces no kind - a verdict the model wrote and never
    // changed is a node like any other under W6.
    let revise = tool(vec![node_added_by(Actor::Agent, "verdict", "Go")]);

    let output = revise
        .run(r#"{"map":"debates","changes":[{"op":"remove_node","node":{"kind":"verdict","name":"Go"},"why":"wrong"}]}"#)
        .unwrap();

    assert!(matches!(output.commits[0], Payload::NodeRemoved { .. }));
}

#[test]
fn a_change_node_op_citing_no_sources_is_refused() {
    let revise = tool(vec![node_added_by(Actor::Agent, "claim", "Rust")]);

    let args = r#"{"map":"debates","changes":[{"op":"change_node","node":{"kind":"claim","name":"Rust"},"properties":{"summary":"fast"},"sources":[]}]}"#;

    let err = revise.run(args).err().expect("refused");

    assert!(err.to_string().contains("cites no sources"), "{err}");
}

#[test]
fn a_change_node_op_renaming_a_verdict_the_model_wrote_is_allowed() {
    // The core enforces no per-kind rule against renaming a verdict -
    // only W6's rank rule, which a model renaming its own untouched
    // node passes.
    let added = node_added_by(Actor::Agent, "verdict", "use axum");
    let source = added.id();
    let revise = tool(vec![added]);

    let args = format!(
        r#"{{"map":"debates","changes":[{{"op":"change_node","node":{{"kind":"verdict","name":"use axum"}},"name":"use actix","sources":["{}"]}}]}}"#,
        source.as_uuid()
    );

    let output = revise.run(&args).unwrap();

    assert!(output.content.contains("use actix"), "{}", output.content);
}
