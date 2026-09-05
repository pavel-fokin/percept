use super::*;
use crate::testing::{source, usage};

fn text(message: &Message) -> &str {
    match message {
        Message::Text { content, .. } => content,
        _ => panic!("expected a Text message"),
    }
}

#[test]
fn a_thought_recorded_event_is_filtered_out_while_a_neighbouring_message_survives() {
    let events = vec![
        Event::message_received(Actor::User, "hi".to_string(), source("tui"), None),
        Event::thought_recorded(
            Actor::Model,
            "let me think".to_string(),
            source("tui"),
            None,
        ),
        Event::message_received(Actor::Model, "done".to_string(), source("tui"), None),
    ];

    let messages = to_messages(&events);

    assert_eq!(messages.len(), 2);
    assert_eq!(text(&messages[0]), "hi");
    assert_eq!(text(&messages[1]), "done");
}

#[test]
fn a_model_called_event_is_filtered_out_while_a_neighbouring_message_survives() {
    let usage = usage();
    let events = vec![
        Event::message_received(Actor::User, "hi".to_string(), source("tui"), None),
        Event::model_called(usage, source("tui"), None),
        Event::message_received(Actor::Model, "done".to_string(), source("tui"), None),
    ];

    let messages = to_messages(&events);

    assert_eq!(messages.len(), 2);
    assert_eq!(text(&messages[0]), "hi");
    assert_eq!(text(&messages[1]), "done");
}

#[test]
fn a_map_change_is_filtered_out_while_a_neighbouring_message_survives() {
    use crate::percept::{EventId, NodeId};
    use std::collections::BTreeMap;

    let node = NodeId::new();
    let events = vec![
        Event::message_received(Actor::User, "hi".to_string(), source("tui"), None),
        Event::new(
            Actor::System,
            source("tui"),
            None,
            Payload::NodeAdded {
                map: "decisions".to_string(),
                node,
                kind: "evidence".to_string(),
                name: "Both built in parallel".to_string(),
                properties: BTreeMap::new(),
                sources: vec![EventId::new()],
            },
        ),
        Event::new(
            Actor::System,
            source("tui"),
            None,
            Payload::EdgeAdded {
                map: "decisions".to_string(),
                kind: "supports".to_string(),
                from: node,
                to: node,
                sources: Vec::new(),
            },
        ),
        Event::message_received(Actor::Model, "done".to_string(), source("tui"), None),
    ];

    let messages = to_messages(&events);

    assert_eq!(messages.len(), 2);
    assert_eq!(text(&messages[0]), "hi");
    assert_eq!(text(&messages[1]), "done");
}

#[test]
fn a_tool_call_and_its_result_replay_as_tool_messages() {
    let events = vec![
        Event::message_received(Actor::User, "search".to_string(), source("tui"), None),
        Event::new(
            Actor::Model,
            source("tui"),
            None,
            Payload::ToolCalled {
                tool: "search_events".to_string(),
                arguments: r#"{"size":5}"#.to_string(),
            },
        ),
        Event::new(
            Actor::System,
            source("tui"),
            None,
            Payload::ToolResulted {
                content: "3 events".to_string(),
            },
        ),
    ];

    let messages = to_messages(&events);

    assert_eq!(messages.len(), 3);
    assert!(matches!(messages[0], Message::Text { .. }));
    match &messages[1] {
        Message::ToolCall { tool, arguments } => {
            assert_eq!(tool, "search_events");
            assert_eq!(arguments, r#"{"size":5}"#);
        }
        _ => panic!("expected a ToolCall message"),
    }
    match &messages[2] {
        Message::ToolResult { content } => assert_eq!(content, "3 events"),
        _ => panic!("expected a ToolResult message"),
    }
}

#[test]
fn a_slice_opening_on_a_tool_result_drops_it() {
    let events = vec![
        Event::new(
            Actor::System,
            source("tui"),
            None,
            Payload::ToolResulted {
                content: "3 events".to_string(),
            },
        ),
        Event::message_received(Actor::Model, "found it".to_string(), source("tui"), None),
    ];

    let messages = to_messages(&events);

    assert_eq!(messages.len(), 1);
    assert_eq!(text(&messages[0]), "found it");
}
