use std::path::PathBuf;

use super::*;
use crate::core::testing::{human, source_at, FakeLog};
use crate::core::{Actor, Event, EventId, Payload};

fn message_at(path: &str, content: &str) -> Event {
    Event::new(
        Actor::Human(human()),
        source_at("test", path),
        None,
        Payload::MessageReceived {
            content: content.to_string(),
        },
    )
}

fn called_at(path: &str) -> Event {
    Event::new(
        Actor::Agent,
        source_at("test", path),
        None,
        Payload::ToolCalled {
            tool: "read_file".to_string(),
            arguments: "{}".to_string(),
        },
    )
}

fn resulted_by(path: &str, cause: EventId) -> Event {
    Event::new(
        Actor::System,
        source_at("test", path),
        Some(cause),
        Payload::ToolResulted {
            content: "ok".to_string(),
        },
    )
}

fn params() -> Params {
    Params {
        since: None,
        until: None,
        kind: None,
        actor: None,
        contains: None,
        size: None,
    }
}

#[test]
fn list_scopes_to_the_given_root_and_reports_the_total_before_size_cuts_it() {
    let log = FakeLog::seeded(vec![
        message_at("/project", "in scope"),
        message_at("/elsewhere", "out of scope"),
    ]);

    let body = list(&log, params(), PathBuf::from("/project")).unwrap();

    assert_eq!(body["total"], 1);
    assert_eq!(body["events"].as_array().unwrap().len(), 1);
}

#[test]
fn list_keeps_only_the_size_most_recent_matches() {
    let events = (0..3).map(|n| message_at("/project", &n.to_string())).collect();
    let log = FakeLog::seeded(events);

    let body = list(
        &log,
        Params {
            size: Some(2),
            ..params()
        },
        PathBuf::from("/project"),
    )
    .unwrap();

    assert_eq!(body["total"], 3);
    assert_eq!(body["events"].as_array().unwrap().len(), 2);
}

#[test]
fn list_rejects_a_blank_contains_value() {
    let log = FakeLog::default();
    let err = list(
        &log,
        Params {
            contains: Some(String::new()),
            ..params()
        },
        PathBuf::from("/project"),
    )
    .expect_err("blank contains is refused");
    assert!(matches!(err, Error::Bad(_)));
}

#[test]
fn list_rejects_an_inverted_window() {
    let log = FakeLog::default();
    let err = list(
        &log,
        Params {
            since: Some("1h".to_string()),
            until: Some("2h".to_string()),
            ..params()
        },
        PathBuf::from("/project"),
    )
    .expect_err("since after until is refused");
    assert!(matches!(err, Error::Bad(_)));
}

#[test]
fn a_kept_calls_answer_is_carried() {
    let call = called_at("/project");
    let result = resulted_by("/project", call.id());
    let log = FakeLog::seeded(vec![call.clone(), result.clone()]);

    let body = list(&log, params(), PathBuf::from("/project")).unwrap();

    assert_eq!(body["events"].as_array().unwrap().len(), 1);
    let carried = body["carried"].as_array().unwrap();
    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0]["id"], result.id().as_uuid().to_string());
}

#[test]
fn text_in_a_tool_result_finds_the_call_that_carries_it() {
    let call = called_at("/project");
    let result = Event::new(
        Actor::System,
        source_at("test", "/project"),
        Some(call.id()),
        Payload::ToolResulted {
            content: format!("{}needle", "x".repeat(200)),
        },
    );
    let log = FakeLog::seeded(vec![call.clone(), result]);

    let body = list(
        &log,
        Params {
            contains: Some("needle".to_string()),
            ..params()
        },
        PathBuf::from("/project"),
    )
    .unwrap();

    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["id"], call.id().as_uuid().to_string());
    assert_eq!(body["carried"][0]["preview"]["match"], 200);
    assert!(body["carried"][0]["payload"]["content"]
        .as_str()
        .unwrap()
        .contains("needle"));
}

#[test]
fn folded_result_text_does_not_override_filters_on_its_call() {
    let call = called_at("/project");
    let result = Event::new(
        Actor::System,
        source_at("test", "/project"),
        Some(call.id()),
        Payload::ToolResulted {
            content: "needle".to_string(),
        },
    );
    let log = FakeLog::seeded(vec![call, result]);

    let body = list(
        &log,
        Params {
            actor: Some("human".to_string()),
            contains: Some("needle".to_string()),
            ..params()
        },
        PathBuf::from("/project"),
    )
    .unwrap();

    assert_eq!(body["total"], 0);
}

#[test]
fn an_answer_whose_call_fell_outside_size_is_not_carried() {
    let call = called_at("/project");
    let result = resulted_by("/project", call.id());
    let later = message_at("/project", "after");
    let log = FakeLog::seeded(vec![call, result, later]);

    let body = list(
        &log,
        Params {
            size: Some(1),
            ..params()
        },
        PathBuf::from("/project"),
    )
    .unwrap();

    assert_eq!(body["events"].as_array().unwrap().len(), 1);
    assert_eq!(body["carried"].as_array().unwrap().len(), 0);
}

#[test]
fn a_tool_resulted_event_never_appears_among_the_events_with_no_filter_at_all() {
    let call = called_at("/project");
    let result = resulted_by("/project", call.id());
    let log = FakeLog::seeded(vec![call, result]);

    let body = list(&log, params(), PathBuf::from("/project")).unwrap();

    let kinds: Vec<_> = body["events"].as_array().unwrap().iter().map(|e| e["type"].as_str().unwrap()).collect();
    assert!(!kinds.contains(&"tool.resulted"));
}

#[test]
fn total_does_not_count_carried_events() {
    let call = called_at("/project");
    let result = resulted_by("/project", call.id());
    let log = FakeLog::seeded(vec![call, result]);

    let body = list(&log, params(), PathBuf::from("/project")).unwrap();

    assert_eq!(body["total"], 1);
}

#[test]
fn get_returns_the_full_event_by_id() {
    let event = message_at("/project", "hello");
    let id = event.id();
    let log = FakeLog::seeded(vec![event]);

    let body = get(&log, &id.as_uuid().to_string(), std::path::Path::new("/project")).unwrap();
    assert_eq!(body["event"]["payload"]["content"], "hello");
}

#[test]
fn get_is_not_found_for_an_id_from_another_project() {
    let event = message_at("/elsewhere", "hello");
    let id = event.id();
    let log = FakeLog::seeded(vec![event]);

    let err = get(&log, &id.as_uuid().to_string(), std::path::Path::new("/project"))
        .expect_err("an id from another project is not found");
    assert!(matches!(err, Error::NotFound(_)));
}

#[test]
fn get_is_bad_for_an_unparseable_id() {
    let log = FakeLog::default();
    let err = get(&log, "not-a-uuid", std::path::Path::new("/project"))
        .expect_err("an unparseable id is bad");
    assert!(matches!(err, Error::Bad(_)));
}
