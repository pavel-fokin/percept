use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

use crate::percept::{self, EventLog};
use crate::store::{Error, Event};

/// A JSONL event log: one compact `store::Event` per line, appended to
/// as the app runs and replayed to rebuild the transcript on start.
/// Implements `percept::EventLog` - `append` flushes after every write
/// but never calls `fsync`, so a killed process loses nothing, but a
/// power cut can lose the tail line.
pub struct Jsonl {
    /// Every read and write goes through this one handle, so the log
    /// can't be read from one file while being written to another -
    /// which is what deleting or replacing the path mid-run would
    /// otherwise produce.
    file: Mutex<fs::File>,
}

impl Jsonl {
    /// Opens the log at `path`, creating its parent directory and the
    /// file itself if either is missing. A missing file is an empty
    /// log, not an error. The returned handle stays open in append mode
    /// for the life of the process.
    ///
    /// One handle per path, and one process per log file. Nothing
    /// enforces this. A second handle repairs the tail (below) with no
    /// knowledge of what the first has half-written, so it can truncate
    /// away a line that was about to be finished.
    ///
    /// A torn final line is truncated here, before anything is
    /// appended. Reads tolerate one, but a later append would land
    /// directly after the partial bytes and fuse into a line that
    /// isn't final - and so isn't tolerated - bricking the log.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(Error::Io)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .map_err(Error::Io)?;
        truncate_torn_tail(&mut file)?;

        Ok(Self {
            file: Mutex::new(file),
        })
    }

    fn parse_line(raw: &str) -> Result<percept::Event, Error> {
        let wire: Event = serde_json::from_str(raw).map_err(Error::BadLine)?;
        percept::Event::try_from(wire)
    }
}

impl EventLog for Jsonl {
    /// Appends one event as a compact JSON line and flushes.
    fn append(&self, event: &percept::Event) -> Result<(), Box<dyn std::error::Error>> {
        let mut line =
            serde_json::to_string(&Event::from(event)).expect("store::Event always serializes");
        line.push('\n');

        let mut file = self.file.lock().expect("jsonl file mutex poisoned");
        if let Err(e) = file.write_all(line.as_bytes()) {
            // A short write leaves a partial line with no newline. The
            // next append would land right after it and fuse into one
            // corrupt line that no longer looks torn, which nothing
            // repairs. Cut it back before returning the failure.
            let _ = truncate_torn_tail(&mut file);
            return Err(Error::Io(e).into());
        }
        file.flush().map_err(Error::Io)?;
        Ok(())
    }

    /// Loads every event in the log, in the order it was appended.
    ///
    /// A malformed line, or one naming an unknown event type or actor,
    /// fails the whole load and names the 1-based line number - per the
    /// ADR, a log written by a newer build is an error, not something
    /// to skip past.
    ///
    /// Anything after the last newline is ignored: an empty tail when
    /// the file ends cleanly, or a torn write from a killed process
    /// when it doesn't. The lock is held throughout, so an `append`
    /// running on another thread can't have its half-written line read
    /// as a torn one and dropped.
    fn load(&self) -> Result<Vec<percept::Event>, Box<dyn std::error::Error>> {
        let mut file = self.file.lock().expect("jsonl file mutex poisoned");
        let bytes = read_all(&mut file)?;

        let complete = match bytes.iter().rposition(|byte| *byte == b'\n') {
            Some(last) => &bytes[..=last],
            None => &[][..],
        };
        // Bytes, not `read_to_string`: a torn write can split a
        // multi-byte character, and that tail is about to be discarded.
        let contents = std::str::from_utf8(complete)
            .map_err(|e| Error::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?;

        let mut events = Vec::new();
        for (idx, raw) in contents.split('\n').enumerate() {
            if raw.is_empty() {
                continue;
            }
            let event = Jsonl::parse_line(raw).map_err(|source| Error::AtLine {
                line: idx + 1,
                source: Box::new(source),
            })?;
            events.push(event);
        }
        Ok(events)
    }
}

fn read_all(file: &mut fs::File) -> Result<Vec<u8>, Error> {
    file.seek(SeekFrom::Start(0)).map_err(Error::Io)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(Error::Io)?;
    Ok(bytes)
}

/// Cuts the file back to its last complete line. Does nothing to a file
/// that's empty or already ends in a newline.
fn truncate_torn_tail(file: &mut fs::File) -> Result<(), Error> {
    let bytes = read_all(file)?;
    if bytes.last().is_none_or(|byte| *byte == b'\n') {
        return Ok(());
    }
    let keep = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |last| last as u64 + 1);
    file.set_len(keep).map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Payload};
    use std::fs::OpenOptions as StdOpenOptions;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!("percept-jsonl-test-{}.jsonl", Uuid::now_v7()))
    }

    fn message(actor: Actor, content: &str, seq: u64) -> percept::Event {
        percept::Event::message_received(actor, content.to_string(), seq, None)
    }

    #[test]
    fn round_trips_several_events_in_order() {
        let path = temp_path();
        let log = Jsonl::open(&path).unwrap();

        let events = vec![
            message(Actor::User, "hi", 0),
            message(Actor::Model, "hello", 1),
            message(Actor::User, "how are you", 2),
        ];
        for event in &events {
            log.append(event).unwrap();
        }

        let loaded = log.load().unwrap();
        assert_eq!(loaded.len(), events.len());
        for (original, restored) in events.iter().zip(loaded.iter()) {
            assert!(restored.id() == original.id());
            assert_eq!(restored.seq(), original.seq());
            assert!(restored.actor() == original.actor());
        }

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn missing_file_loads_empty() {
        let path = temp_path();
        assert!(!path.exists());

        let log = Jsonl::open(&path).unwrap();
        assert!(log.load().unwrap().is_empty());

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn torn_final_line_is_dropped() {
        let path = temp_path();
        let log = Jsonl::open(&path).unwrap();

        log.append(&message(Actor::User, "complete line", 0))
            .unwrap();

        // Simulate a process killed mid-write: a second line with no
        // trailing newline.
        let wire = Event::from(&message(Actor::Model, "torn", 1));
        let partial = serde_json::to_string(&wire).unwrap();
        let mut file = StdOpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&partial.as_bytes()[..partial.len() / 2])
            .unwrap();
        file.flush().unwrap();
        drop(file);

        let loaded = log.load().unwrap();
        assert_eq!(loaded.len(), 1);
        match loaded[0].payload() {
            Payload::MessageReceived { content } => assert_eq!(content, "complete line"),
        }

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn malformed_mid_file_line_fails_with_its_line_number() {
        let path = temp_path();
        let log = Jsonl::open(&path).unwrap();

        log.append(&message(Actor::User, "first", 0)).unwrap();

        let mut file = StdOpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "not json").unwrap();
        file.flush().unwrap();
        drop(file);

        log.append(&message(Actor::User, "third", 2)).unwrap();

        match log.load() {
            Err(err) => match err.downcast_ref::<Error>() {
                Some(Error::AtLine { line, source }) => {
                    assert_eq!(*line, 2);
                    assert!(matches!(**source, Error::BadLine(_)));
                }
                _ => panic!("expected Error::AtLine"),
            },
            _ => panic!("expected Err(AtLine)"),
        }

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn empty_lines_are_skipped() {
        let path = temp_path();
        let log = Jsonl::open(&path).unwrap();

        let event = message(Actor::User, "hi", 0);
        let wire = Event::from(&event);
        let line = serde_json::to_string(&wire).unwrap();
        fs::write(&path, format!("\n{line}\n\n")).unwrap();

        let loaded = log.load().unwrap();
        assert_eq!(loaded.len(), 1);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn open_creates_missing_parent_directory() {
        let dir = std::env::temp_dir().join(format!("percept-jsonl-dir-{}", Uuid::now_v7()));
        let path = dir.join("log.jsonl");
        assert!(!dir.exists());

        let log = Jsonl::open(&path).unwrap();
        log.append(&message(Actor::User, "hi", 0)).unwrap();
        assert_eq!(log.load().unwrap().len(), 1);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reopening_after_a_torn_write_drops_the_partial_line() {
        let path = temp_path();
        let log = Jsonl::open(&path).unwrap();
        log.append(&message(Actor::User, "complete line", 0))
            .unwrap();

        let wire = Event::from(&message(Actor::Model, "torn", 1));
        let partial = serde_json::to_string(&wire).unwrap();
        let mut file = StdOpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&partial.as_bytes()[..partial.len() / 2])
            .unwrap();
        drop(file);
        drop(log);

        // The next run appends after the torn bytes. Without truncation
        // they'd fuse into one unreadable line.
        let reopened = Jsonl::open(&path).unwrap();
        reopened.append(&message(Actor::User, "next run", 1)).unwrap();

        let loaded = reopened.load().unwrap();
        assert_eq!(loaded.len(), 2);
        match loaded[1].payload() {
            Payload::MessageReceived { content } => assert_eq!(content, "next run"),
        }

        fs::remove_file(&path).unwrap();
    }
}
