use super::*;
use crate::percept::{Actor, EventQuery, Payload};
use crate::testing::source;
use std::path::PathBuf;
use uuid::Uuid;

/// A log file on a temp path, removed when the test ends - a
/// trailing `remove_file` never runs on the failing test, which is
/// the one whose file you'd want gone.
struct TempLog {
    path: PathBuf,
}

impl TempLog {
    fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("percept-jsonl-{}.jsonl", Uuid::now_v7())),
        }
    }

    /// A handle on the file - call it twice to stand in for a
    /// restart.
    fn open(&self) -> Jsonl {
        Jsonl::open(&self.path).unwrap()
    }

    fn write_raw(&self, text: &str) {
        let mut file = OpenOptions::new().append(true).open(&self.path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    /// Half a line, no newline: a process killed mid-write.
    fn write_torn_tail(&self) {
        let line = line(&message(Actor::Model, "torn"));
        self.write_raw(&line[..line.len() / 2]);
    }
}

impl Drop for TempLog {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn message(actor: Actor, content: &str) -> percept::Event {
    percept::Event::message_received(actor, content.to_string(), source("tui"), None)
}

fn line(event: &percept::Event) -> String {
    serde_json::to_string(&Event::from(event)).unwrap()
}

fn content(event: &percept::Event) -> &str {
    match event.payload() {
        Payload::MessageReceived { content } => content,
        _ => panic!("expected a message.received event"),
    }
}

#[test]
fn get_finds_one_event_by_id_and_reports_a_missing_one_as_absent() {
    let temp = TempLog::new();
    let log = temp.open();

    let wanted = message(Actor::Model, "hello");
    for event in [message(Actor::User, "hi"), wanted.clone()] {
        log.append(&event).unwrap();
    }

    let found = log.get(wanted.id()).unwrap().expect("appended event");
    assert!(found.id() == wanted.id());
    assert!(log.get(percept::EventId::new()).unwrap().is_none());
}

#[test]
fn round_trips_several_events_in_order() {
    let temp = TempLog::new();
    let log = temp.open();

    let events = vec![
        message(Actor::User, "hi"),
        message(Actor::Model, "hello"),
        message(Actor::User, "how are you"),
    ];
    for event in &events {
        log.append(event).unwrap();
    }

    let loaded = log.load().unwrap();
    assert_eq!(loaded.len(), events.len());
    for (original, restored) in events.iter().zip(loaded.iter()) {
        assert!(restored.id() == original.id());
        assert_eq!(restored.source(), original.source());
        assert!(restored.actor() == original.actor());
    }
}

#[test]
fn missing_file_loads_empty() {
    let temp = TempLog::new();
    assert!(!temp.path.exists());

    assert!(temp.open().load().unwrap().is_empty());
}

#[test]
fn torn_final_line_is_dropped() {
    let temp = TempLog::new();
    let log = temp.open();
    log.append(&message(Actor::User, "complete line")).unwrap();
    temp.write_torn_tail();

    let loaded = log.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(content(&loaded[0]), "complete line");
}

#[test]
fn reopening_after_a_torn_write_drops_the_partial_line() {
    let temp = TempLog::new();
    let log = temp.open();
    log.append(&message(Actor::User, "complete line")).unwrap();
    temp.write_torn_tail();
    drop(log);

    // The next run appends after the torn bytes. Without truncation
    // they'd fuse into one unreadable line.
    let reopened = temp.open();
    reopened.append(&message(Actor::User, "next run")).unwrap();

    let loaded = reopened.load().unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(content(&loaded[1]), "next run");
}

#[test]
fn malformed_mid_file_line_fails_with_its_line_number() {
    let temp = TempLog::new();
    let log = temp.open();

    log.append(&message(Actor::User, "first")).unwrap();
    temp.write_raw("not json\n");
    log.append(&message(Actor::User, "third")).unwrap();

    let Err(err) = log.load() else {
        panic!("a malformed line must fail the load");
    };
    match err.downcast_ref::<Error>() {
        Some(Error::AtLine { line, source }) => {
            assert_eq!(*line, 2);
            assert!(matches!(**source, Error::BadLine(_)));
        }
        _ => panic!("expected Error::AtLine, got {err}"),
    }
}

#[test]
fn an_events_source_name_and_path_round_trip_through_the_store() {
    let temp = TempLog::new();
    let log = temp.open();

    let written = percept::Event::message_received(
        Actor::User,
        "hi".to_string(),
        percept::Source {
            name: "claude-code".to_string(),
            path: PathBuf::from("/home/pavel/project"),
        },
        None,
    );
    log.append(&written).unwrap();

    let loaded = log.load().unwrap();
    assert_eq!(loaded[0].source().name, "claude-code");
    assert_eq!(
        loaded[0].source().path,
        PathBuf::from("/home/pavel/project")
    );
}

#[test]
fn empty_lines_are_skipped() {
    let temp = TempLog::new();
    let log = temp.open();

    let event = message(Actor::User, "hi");
    fs::write(&temp.path, format!("\n{}\n\n", line(&event))).unwrap();

    assert_eq!(log.load().unwrap().len(), 1);
}

#[test]
fn two_handles_on_one_path_share_the_log() {
    let temp = TempLog::new();
    let first = temp.open();
    // A second handle must not block: if any operation held the
    // lock past its own scope, this would hang instead of failing.
    let second = temp.open();

    first
        .append(&message(Actor::User, "from the first"))
        .unwrap();
    second
        .append(&message(Actor::Model, "from the second"))
        .unwrap();

    let loaded = second.load().unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(first.load().unwrap().len(), 2);
}

#[test]
fn open_creates_missing_parent_directory() {
    let dir = std::env::temp_dir().join(format!("percept-jsonl-dir-{}", Uuid::now_v7()));
    let path = dir.join("log.jsonl");
    assert!(!dir.exists());

    let log = Jsonl::open(&path).unwrap();
    log.append(&message(Actor::User, "hi")).unwrap();
    assert_eq!(log.load().unwrap().len(), 1);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn search_reads_the_file_and_applies_the_query() {
    let temp = TempLog::new();
    let log = temp.open();
    log.append(&message(Actor::User, "hi")).unwrap();
    log.append(&message(Actor::Model, "hello")).unwrap();

    let query = EventQuery {
        actors: vec![Actor::Model],
        ..Default::default()
    };
    let found = log.search(&query).unwrap();

    assert_eq!(found.len(), 1);
    assert!(found[0].actor() == Actor::Model);
}
