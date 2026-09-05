use super::*;
use crate::percept::{Actor, Event, EventId, Payload};
use crate::testing::source;
use std::sync::Mutex;

/// The filters a test asserts `run` translated correctly.
#[derive(Default)]
struct Seen {
    actors: Vec<Actor>,
    text: Vec<String>,
    size: Option<usize>,
}

#[derive(Default)]
struct FakeSearch {
    events: Vec<Event>,
    seen: Mutex<Seen>,
}

impl EventSearch for FakeSearch {
    fn search(&self, query: &EventQuery) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
        *self.seen.lock().unwrap() = Seen {
            actors: query.actors.clone(),
            text: query.text.clone(),
            size: query.size,
        };
        Ok(query.apply(self.events.clone()))
    }
}

fn message(name: &str, content: &str) -> Event {
    Event::restore(
        EventId::new(),
        Actor::User,
        source(name),
        None,
        Timestamp::now(),
        Payload::MessageReceived {
            content: content.to_string(),
        },
    )
}

fn tool() -> SearchEvents {
    SearchEvents::new(Arc::new(FakeSearch {
        events: vec![message("tui", "hello"), message("claude-code", "world")],
        ..Default::default()
    }))
}

#[test]
fn spec_names_the_tool_and_carries_valid_schema_json() {
    let spec = tool().spec();
    assert_eq!(spec.name, "search_events");
    let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
    assert_eq!(schema["type"], "object");
}

#[test]
fn run_returns_one_summarized_line_per_match() {
    let out = tool().run(r#"{"sources":["tui"]}"#).unwrap();
    let lines: Vec<&str> = out.content.lines().collect();
    assert_eq!(lines.len(), 1);
    let line: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(line["source"]["name"], "tui");
    assert_eq!(line["payload"]["content"], "hello");
}

#[test]
fn run_translates_string_filters_into_domain_enums() {
    let search = Arc::new(FakeSearch {
        events: vec![message("tui", "hi")],
        ..Default::default()
    });
    let tool = SearchEvents::new(search.clone());

    tool.run(r#"{"actors":["user"],"contains":["deploy"],"size":3}"#)
        .unwrap();

    let seen = search.seen.lock().unwrap();
    assert!(seen.actors == vec![Actor::User]);
    assert_eq!(seen.text, vec!["deploy".to_string()]);
    assert_eq!(seen.size, Some(3));
}

#[test]
fn an_empty_object_searches_with_only_the_default_size() {
    let search = Arc::new(FakeSearch {
        events: vec![message("tui", "a"), message("tui", "b")],
        ..Default::default()
    });
    let out = SearchEvents::new(search.clone()).run("{}").unwrap();

    assert_eq!(out.content.lines().count(), 2);
    assert_eq!(search.seen.lock().unwrap().size, Some(DEFAULT_SIZE));
}

#[test]
fn malformed_arguments_are_an_error() {
    assert!(tool().run("not json").is_err());
}

#[test]
fn an_unknown_actor_is_an_error() {
    assert!(tool().run(r#"{"actors":["robot"]}"#).is_err());
}

#[test]
fn a_non_iso_timestamp_is_an_error() {
    assert!(tool().run(r#"{"since":"yesterday"}"#).is_err());
}

#[test]
fn an_empty_bound_is_no_bound() {
    let out = tool().run(r#"{"since":"","until":""}"#).unwrap();
    assert_eq!(out.content.lines().count(), 2);
}

#[test]
fn preview_sizes_the_content_window_and_zero_is_an_error() {
    let out = tool().run(r#"{"preview":2}"#).unwrap();
    let line: serde_json::Value =
        serde_json::from_str(out.content.lines().next().unwrap()).unwrap();
    assert_eq!(line["payload"]["content"], "he\u{2026}");
    assert_eq!(line["preview"]["len"], 5);
    assert!(tool().run(r#"{"preview":0}"#).is_err());
}

#[test]
fn an_unknown_argument_is_an_error() {
    assert!(tool().run(r#"{"limit":3}"#).is_err());
}

#[test]
fn a_blank_contains_term_is_an_error() {
    assert!(tool().run(r#"{"contains":[""]}"#).is_err());
}

#[test]
fn run_keeps_only_events_whose_payload_carries_the_term() {
    let out = tool().run(r#"{"contains":["orl"]}"#).unwrap();

    let lines: Vec<&str> = out.content.lines().collect();
    assert_eq!(lines.len(), 1);
    let line: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(line["source"]["name"], "claude-code");
}
