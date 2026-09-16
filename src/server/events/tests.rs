use std::path::PathBuf;

use super::*;
use crate::core::testing::{human, source_at, FakeLog};
use crate::core::{Actor, Event, Payload};

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

fn params() -> Params {
    Params {
        since: None,
        until: None,
        kind: None,
        contains: None,
        size: None,
        preview: None,
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
    .err()
    .expect("blank contains is refused");
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
    .err()
    .expect("since after until is refused");
    assert!(matches!(err, Error::Bad(_)));
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
        .err()
        .expect("an id from another project is not found");
    assert!(matches!(err, Error::NotFound(_)));
}

#[test]
fn get_is_bad_for_an_unparseable_id() {
    let log = FakeLog::default();
    let err = get(&log, "not-a-uuid", std::path::Path::new("/project"))
        .err()
        .expect("an unparseable id is bad");
    assert!(matches!(err, Error::Bad(_)));
}
