use super::*;
use crate::percept::{Actor, Event, EventId, Payload};
use crate::shared::Timestamp;
use crate::testing::{source, FakeLog};

fn message(content: &str) -> Event {
    Event::message_received(Actor::User, content.to_string(), source("tui"), None)
}

#[test]
fn spec_names_the_tool_and_carries_valid_schema_json() {
    let tool = ReadEvent::new(Arc::new(FakeLog::default()));
    let spec = tool.spec();
    assert_eq!(spec.name, "read_event");
    let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
    assert_eq!(schema["type"], "object");
}

#[test]
fn missing_id_is_an_error() {
    let tool = ReadEvent::new(Arc::new(FakeLog::default()));
    assert!(tool.run("{}").is_err());
}

#[test]
fn an_unknown_id_is_an_error() {
    let tool = ReadEvent::new(Arc::new(FakeLog::default()));
    let unknown = EventId::new().as_uuid().to_string();
    assert!(tool.run(&format!(r#"{{"id":"{unknown}"}}"#)).is_err());
}

#[test]
fn no_range_returns_the_whole_event_as_show_prints_it() {
    let event = message("hello world");
    let log = Arc::new(FakeLog::seeded(vec![event.clone()]));
    let tool = ReadEvent::new(log);

    let out = tool
        .run(&format!(r#"{{"id":"{}"}}"#, event.id().as_uuid()))
        .unwrap();
    assert_eq!(out.content, encode(&event));
}

#[test]
fn an_end_past_the_length_clamps_to_it() {
    let event = message("hi");
    let log = Arc::new(FakeLog::seeded(vec![event.clone()]));
    let tool = ReadEvent::new(log);

    let out = tool
        .run(&format!(
            r#"{{"id":"{}","end":9000}}"#,
            event.id().as_uuid()
        ))
        .unwrap();
    let line: serde_json::Value = serde_json::from_str(&out.content).unwrap();
    assert_eq!(line["payload"]["content"], "hi");
    assert_eq!(line["preview"]["len"], 2);
}

#[test]
fn an_unknown_argument_is_an_error() {
    let event = message("hello");
    let tool = ReadEvent::new(Arc::new(FakeLog::seeded(vec![event.clone()])));
    let err = tool
        .run(&format!(
            r#"{{"id":"{}","range":"0:2"}}"#,
            event.id().as_uuid()
        ))
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("range"), "{err}");
}

#[test]
fn a_start_past_the_end_is_an_error() {
    let event = message("hello");
    let log = Arc::new(FakeLog::seeded(vec![event.clone()]));
    let tool = ReadEvent::new(log);

    assert!(tool
        .run(&format!(
            r#"{{"id":"{}","start":9000}}"#,
            event.id().as_uuid()
        ))
        .is_err());
}

#[test]
fn a_range_on_a_tool_called_event_is_an_error() {
    let call = Event::restore(
        EventId::new(),
        Actor::Model,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::ToolCalled {
            tool: "search_events".to_string(),
            arguments: "{}".to_string(),
        },
    );
    let log = Arc::new(FakeLog::seeded(vec![call.clone()]));
    let tool = ReadEvent::new(log);

    assert!(tool
        .run(&format!(r#"{{"id":"{}","start":0}}"#, call.id().as_uuid()))
        .is_err());
}
