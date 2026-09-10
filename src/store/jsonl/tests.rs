use super::*;
use crate::core::testing::{human, source};
use crate::core::{Actor, EventQuery, NodeId, Payload};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// A log file under its own temp directory, removed when the test ends -
/// a trailing `remove_dir_all` never runs on the failing test, which is
/// the one whose files you'd want gone. Its own directory, not a bare
/// file in the shared temp dir, so this log's `me` file - `Jsonl::open`
/// keeps it beside the log - never collides with another `TempLog`'s.
struct TempLog {
    dir: PathBuf,
    path: PathBuf,
}

impl TempLog {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("percept-jsonl-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            path: dir.join("percept.jsonl"),
            dir,
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
        let line = line(&message(Actor::Agent, "torn"));
        self.write_raw(&line[..line.len() / 2]);
    }
}

impl Drop for TempLog {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn message(actor: Actor, content: &str) -> crate::core::Event {
    crate::core::Event::message_received(actor, content.to_string(), source("tui"), None)
}

fn line(event: &crate::core::Event) -> String {
    serde_json::to_string(&Event::from(event)).unwrap()
}

fn content(event: &crate::core::Event) -> &str {
    match event.payload() {
        Payload::MessageReceived { content } => content,
        _ => panic!("expected a message.received event"),
    }
}

#[test]
fn get_finds_one_event_by_id_and_reports_a_missing_one_as_absent() {
    let temp = TempLog::new();
    let log = temp.open();

    let wanted = message(Actor::Agent, "hello");
    for event in [message(Actor::Human(human()), "hi"), wanted.clone()] {
        log.append(&event).unwrap();
    }

    let found = log.get(wanted.id()).unwrap().expect("appended event");
    assert!(found.id() == wanted.id());
    assert!(log.get(crate::core::EventId::new()).unwrap().is_none());
}

#[test]
fn round_trips_several_events_in_order() {
    let temp = TempLog::new();
    let log = temp.open();

    let events = vec![
        message(Actor::Human(human()), "hi"),
        message(Actor::Agent, "hello"),
        message(Actor::Human(human()), "how are you"),
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
    log.append(&message(Actor::Human(human()), "complete line")).unwrap();
    temp.write_torn_tail();

    let loaded = log.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(content(&loaded[0]), "complete line");
}

#[test]
fn reopening_after_a_torn_write_drops_the_partial_line() {
    let temp = TempLog::new();
    let log = temp.open();
    log.append(&message(Actor::Human(human()), "complete line")).unwrap();
    temp.write_torn_tail();
    drop(log);

    // The next run appends after the torn bytes. Without truncation
    // they'd fuse into one unreadable line.
    let reopened = temp.open();
    reopened.append(&message(Actor::Human(human()), "next run")).unwrap();

    let loaded = reopened.load().unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(content(&loaded[1]), "next run");
}

#[test]
fn malformed_mid_file_line_fails_with_its_line_number() {
    let temp = TempLog::new();
    let log = temp.open();

    log.append(&message(Actor::Human(human()), "first")).unwrap();
    temp.write_raw("not json\n");
    log.append(&message(Actor::Human(human()), "third")).unwrap();

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

    let written = crate::core::Event::message_received(
        Actor::Human(human()),
        "hi".to_string(),
        crate::core::Source {
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

    let event = message(Actor::Human(human()), "hi");
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
        .append(&message(Actor::Human(human()), "from the first"))
        .unwrap();
    second
        .append(&message(Actor::Agent, "from the second"))
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
    log.append(&message(Actor::Human(human()), "hi")).unwrap();
    assert_eq!(log.load().unwrap().len(), 1);

    fs::remove_dir_all(&dir).unwrap();
}

/// Counts the `node.added` events of `kind` already in `events` and
/// builds one more, minted with the next number - the same count
/// `Map::apply` would take, done here without a `Map` so the test
/// stays a seam-level check on `append_computed` alone.
fn mint(events: Vec<crate::core::Event>, kind: &str) -> Result<crate::core::Event, Box<dyn std::error::Error>> {
    let seq = events
        .iter()
        .filter(|event| matches!(event.payload(), Payload::NodeAdded { kind: k, .. } if k == kind))
        .count() as u32
        + 1;
    Ok(crate::core::Event::new(
        Actor::Human(human()),
        source("cli"),
        None,
        Payload::NodeAdded {
            map: "decisions".to_string(),
            node: NodeId::new(),
            kind: kind.to_string(),
            name: format!("mint {seq}"),
            properties: BTreeMap::new(),
            sources: Vec::new(),
            seq,
        },
    ))
}

#[test]
fn concurrent_mints_of_the_same_kind_never_agree_on_a_number() {
    let temp = TempLog::new();
    let log = Arc::new(temp.open());
    let threads = 16;

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                log.append_computed(Box::new(|events| mint(events, "decision")))
                    .unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }

    let minted: Vec<u32> = log
        .load()
        .unwrap()
        .iter()
        .map(|event| match event.payload() {
            Payload::NodeAdded { seq, .. } => *seq,
            _ => panic!("expected a node.added event"),
        })
        .collect();

    let mut sorted = minted.clone();
    sorted.sort_unstable();
    let expected: Vec<u32> = (1..=threads as u32).collect();
    assert_eq!(sorted, expected, "every mint got a distinct, dense number");
}

#[test]
fn search_reads_the_file_and_applies_the_query() {
    let temp = TempLog::new();
    let log = temp.open();
    log.append(&message(Actor::Human(human()), "hi")).unwrap();
    log.append(&message(Actor::Agent, "hello")).unwrap();

    let query = EventQuery {
        actors: vec![Actor::Agent],
        ..Default::default()
    };
    let found = log.search(&query).unwrap();

    assert_eq!(found.len(), 1);
    assert!(found[0].actor() == Actor::Agent);
}

#[test]
fn opening_a_log_mints_no_me_file_and_reports_no_human_id() {
    let temp = TempLog::new();

    let log = temp.open();

    assert_eq!(log.me(), None);
    assert!(!temp.dir.join("me").exists());
}

#[test]
fn me_is_read_from_the_file_beside_the_log() {
    let temp = TempLog::new();
    let id = Uuid::now_v7();
    std::fs::write(temp.dir.join("me"), format!("{id}\n")).unwrap();

    let log = temp.open();

    assert_eq!(log.me().map(|me| me.as_uuid()), Some(id));
}

#[test]
fn a_me_file_that_is_not_one_uuid_is_an_error() {
    let temp = TempLog::new();
    std::fs::write(temp.dir.join("me"), "not a uuid\n").unwrap();

    assert!(matches!(Jsonl::open(&temp.path), Err(Error::BadMeFile(_))));
}

fn log_id_file(temp: &TempLog) -> PathBuf {
    temp.dir.join("log-id")
}

/// The uuid the `log-id` file beside `temp`'s log holds.
fn log_id(temp: &TempLog) -> String {
    fs::read_to_string(log_id_file(temp)).unwrap().trim().to_string()
}

#[test]
fn opening_a_log_mints_a_log_id_file_and_reopening_reads_the_same_id() {
    let temp = TempLog::new();

    let _first = temp.open();
    let minted = log_id(&temp);
    Uuid::parse_str(&minted).unwrap();

    let _second = temp.open();
    assert_eq!(log_id(&temp), minted);
}

#[test]
fn a_log_id_file_that_is_not_one_uuid_is_an_error_naming_the_path() {
    let temp = TempLog::new();
    std::fs::create_dir_all(&temp.dir).unwrap();
    std::fs::write(log_id_file(&temp), "not a uuid\n").unwrap();

    match Jsonl::open(&temp.path) {
        Err(Error::BadLogIdFile(path)) => assert_eq!(path, log_id_file(&temp)),
        Err(other) => panic!("expected Error::BadLogIdFile, got {other}"),
        Ok(_) => panic!("expected Error::BadLogIdFile, got Ok"),
    }
}

#[test]
fn the_first_appended_line_carries_this_logs_id_and_seq_one_the_second_seq_two() {
    let temp = TempLog::new();
    let log = temp.open();

    log.append(&message(Actor::Human(human()), "first")).unwrap();
    log.append(&message(Actor::Agent, "second")).unwrap();

    let text = fs::read_to_string(&temp.path).unwrap();
    let raw: Vec<&str> = text.lines().collect();
    assert_eq!(raw.len(), 2);

    let first: Event = serde_json::from_str(raw[0]).unwrap();
    let cursor = first.log.unwrap();
    assert_eq!(cursor.id, log_id(&temp));
    assert_eq!(cursor.seq, 1);

    let second: Event = serde_json::from_str(raw[1]).unwrap();
    let cursor = second.log.unwrap();
    assert_eq!(cursor.id, log_id(&temp));
    assert_eq!(cursor.seq, 2);
}

#[test]
fn an_append_after_legacy_lines_gets_the_next_seq() {
    let temp = TempLog::new();
    let log = temp.open();

    for content in ["first", "second", "third"] {
        temp.write_raw(&format!("{}\n", line(&message(Actor::Human(human()), content))));
    }

    log.append(&message(Actor::Agent, "fourth")).unwrap();

    let text = fs::read_to_string(&temp.path).unwrap();
    let raw: Vec<&str> = text.lines().collect();
    assert_eq!(raw.len(), 4);

    let last: Event = serde_json::from_str(raw[3]).unwrap();
    assert_eq!(last.log.unwrap().seq, 4);
}

#[test]
fn a_batch_from_append_batch_computed_gets_consecutive_seqs() {
    let temp = TempLog::new();
    let log = temp.open();

    let batch = log
        .append_batch_computed(Box::new(|_events| {
            Ok(vec![
                message(Actor::Human(human()), "one"),
                message(Actor::Agent, "two"),
                message(Actor::Agent, "three"),
            ])
        }))
        .unwrap();
    assert_eq!(batch.len(), 3);

    let text = fs::read_to_string(&temp.path).unwrap();
    let seqs: Vec<u64> = text
        .lines()
        .map(|raw| serde_json::from_str::<Event>(raw).unwrap().log.unwrap().seq)
        .collect();
    assert_eq!(seqs, vec![1, 2, 3]);
}

#[test]
fn two_handles_on_one_path_never_write_the_same_seq() {
    let temp = TempLog::new();
    let logs = [Arc::new(temp.open()), Arc::new(temp.open())];
    let threads = 16;

    let handles: Vec<_> = (0..threads)
        .map(|i| {
            let log = Arc::clone(&logs[i % 2]);
            std::thread::spawn(move || {
                log.append(&message(Actor::Agent, "concurrent")).unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }

    let text = fs::read_to_string(&temp.path).unwrap();
    let mut seqs: Vec<u64> = text
        .lines()
        .map(|raw| serde_json::from_str::<Event>(raw).unwrap().log.unwrap().seq)
        .collect();
    seqs.sort_unstable();
    let expected: Vec<u64> = (1..=threads as u64).collect();
    assert_eq!(seqs, expected, "every append got a distinct, dense seq");
}

#[test]
fn a_legacy_line_still_loads() {
    let temp = TempLog::new();
    let log = temp.open();
    temp.write_raw(&format!("{}\n", line(&message(Actor::Human(human()), "legacy"))));

    let loaded = log.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(content(&loaded[0]), "legacy");
}
