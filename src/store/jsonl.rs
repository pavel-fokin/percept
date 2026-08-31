use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

use crate::percept::{self, EventLog};
use crate::store::{Error, Event};

/// A JSONL event log: one compact `store::Event` per line, appended to
/// as the app runs and replayed to rebuild the transcript on start.
/// Implements `percept::EventLog`. Each append is one unbuffered write
/// with no `fsync`: a killed process loses nothing, since the bytes are
/// already the kernel's, but a power cut can lose the tail line.
///
/// Several processes may share one log. Every operation holds the
/// file's advisory lock, so none of them observes another mid-write.
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
    /// A tail with no newline is repaired under the lock, so it can
    /// only be a dead writer's leftovers. Without the lock a second
    /// process could not tell that from a line still being written,
    /// and would cut away bytes another writer was about to finish.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(Error::Io)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .map_err(Error::Io)?;
        {
            let _lock = Lock::exclusive(&file)?;
            truncate_torn_tail(&file)?;
        }

        Ok(Self {
            file: Mutex::new(file),
        })
    }

    /// Runs `f` holding both locks: the mutex that orders this
    /// process's threads, and the file lock that orders processes.
    /// Every operation goes through here, so none can be written that
    /// forgets one.
    fn with_exclusive<T>(&self, f: impl FnOnce(&File) -> Result<T, Error>) -> Result<T, Error> {
        let file = self.file.lock().expect("jsonl file mutex poisoned");
        let _lock = Lock::exclusive(&file)?;
        f(&file)
    }

    /// As `with_exclusive`, but readers only exclude writers, not each
    /// other.
    fn with_shared<T>(&self, f: impl FnOnce(&File) -> Result<T, Error>) -> Result<T, Error> {
        let file = self.file.lock().expect("jsonl file mutex poisoned");
        let _lock = Lock::shared(&file)?;
        f(&file)
    }
}

impl EventLog for Jsonl {
    /// Appends one event as a compact JSON line.
    fn append(&self, event: &percept::Event) -> Result<(), Box<dyn std::error::Error>> {
        let mut line =
            serde_json::to_string(&Event::from(event)).expect("store::Event always serializes");
        line.push('\n');

        self.with_exclusive(|file| {
            // One mechanism for every torn tail, whoever left it: a
            // dead writer, a write that failed here last time, or an
            // append by something that never took the lock. The common
            // case reads a single byte.
            truncate_torn_tail(file)?;
            let mut file = file;
            file.write_all(line.as_bytes()).map_err(Error::Io)
        })?;
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
    /// when it doesn't. Both locks are held throughout, so an `append`
    /// running on another thread or in another process can't have its
    /// half-written line read as a torn one and dropped.
    fn load(&self) -> Result<Vec<percept::Event>, Box<dyn std::error::Error>> {
        let bytes = self.with_shared(read_all)?;

        // Bytes, not `read_to_string`: a torn write can split a
        // multi-byte character, and that tail is about to be discarded.
        let contents = std::str::from_utf8(&bytes[..complete_len(&bytes)])
            .map_err(|e| Error::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?;

        let mut events = Vec::new();
        for (idx, raw) in contents.split('\n').enumerate() {
            if raw.is_empty() {
                continue;
            }
            let event = parse_line(raw).map_err(|source| Error::AtLine {
                line: idx + 1,
                source: Box::new(source),
            })?;
            events.push(event);
        }
        Ok(events)
    }
}

fn parse_line(raw: &str) -> Result<percept::Event, Error> {
    let wire: Event = serde_json::from_str(raw).map_err(Error::BadLine)?;
    percept::Event::try_from(wire)
}

fn read_all(mut file: &File) -> Result<Vec<u8>, Error> {
    file.seek(SeekFrom::Start(0)).map_err(Error::Io)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(Error::Io)?;
    Ok(bytes)
}

/// Holds the log's advisory lock until dropped. Taken for every open,
/// append, and load, so the file is only ever seen between whole
/// lines - the invariant a repair depends on to tell a dead writer's
/// tail from a live writer's.
///
/// Advisory, so it binds only processes that ask for it, and it is
/// unreliable on network filesystems.
struct Lock<'a>(&'a File);

impl<'a> Lock<'a> {
    fn exclusive(file: &'a File) -> Result<Self, Error> {
        file.lock().map_err(Error::Io)?;
        Ok(Self(file))
    }

    fn shared(file: &'a File) -> Result<Self, Error> {
        file.lock_shared().map_err(Error::Io)?;
        Ok(Self(file))
    }
}

impl Drop for Lock<'_> {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// How much of `bytes` is complete lines - everything up to and
/// including the last newline. The file format's one invariant, so
/// reading and repairing both ask here.
fn complete_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |last| last + 1)
}

/// Cuts the file back to its last complete line. A tail with no
/// newline is a write that was killed part-way: reads ignore it, but a
/// later append would land right after those bytes and fuse into a
/// line that no longer looks torn, which nothing would ever repair.
///
/// Does nothing to a file that's empty or already ends in a newline -
/// the case on every clean start, which is why it costs a seek and one
/// byte rather than a read of the whole log.
fn truncate_torn_tail(mut file: &File) -> Result<(), Error> {
    let len = file.seek(SeekFrom::End(0)).map_err(Error::Io)?;
    if len == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::End(-1)).map_err(Error::Io)?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last).map_err(Error::Io)?;
    if last[0] == b'\n' {
        return Ok(());
    }

    let keep = last_line_end(file, len)?;
    file.set_len(keep).map_err(Error::Io)
}

/// Where the log's last complete line ends, found by scanning back in
/// growing windows. Only the tail can be torn, so reading the whole
/// file to locate one newline would cost more the longer the log gets -
/// and this runs on every append.
fn last_line_end(mut file: &File, len: u64) -> Result<u64, Error> {
    let mut window = 64 * 1024;
    loop {
        let start = len.saturating_sub(window);
        file.seek(SeekFrom::Start(start)).map_err(Error::Io)?;
        let mut bytes = vec![0u8; (len - start) as usize];
        file.read_exact(&mut bytes).map_err(Error::Io)?;

        let complete = complete_len(&bytes) as u64;
        if complete > 0 || start == 0 {
            return Ok(start + complete);
        }
        window *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Payload};
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
        percept::Event::message_received(actor, content.to_string(), "tui".to_string(), None)
    }

    fn line(event: &percept::Event) -> String {
        serde_json::to_string(&Event::from(event)).unwrap()
    }

    fn content(event: &percept::Event) -> &str {
        match event.payload() {
            Payload::MessageReceived { content } => content,
        }
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
}
