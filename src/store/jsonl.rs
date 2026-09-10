use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

use serde::Deserialize;
use uuid::Uuid;

use crate::core::{EventId, EventLog, EventQuery, EventSearch, HumanId};
use crate::shared::Id;
use crate::store::{parse_event_id, Error, Event};

/// The file beside the log holding this `$PERCEPT_HOME`'s `HumanId` -
/// one UUID and a trailing newline, editable by hand.
const ME_FILE: &str = "me";

/// A JSONL event log: one compact `store::Event` per line, appended to
/// as the app runs and replayed to rebuild the transcript on start.
/// Implements `core::EventLog`. Each append is one unbuffered write
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
    /// This `$PERCEPT_HOME`'s `HumanId`, read from `me` beside the log,
    /// or minted into it on the first open. Every event and node this
    /// process writes as the human, and every legacy `"user"` actor a
    /// line predating the object form is read back as, resolves to it.
    me: HumanId,
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

        let me = open_me(&path.with_file_name(ME_FILE))?;

        Ok(Self {
            file: Mutex::new(file),
            me,
        })
    }

    /// This `$PERCEPT_HOME`'s `HumanId` - the person at the keyboard,
    /// minted once into `me` beside the log and read back here ever
    /// after.
    pub fn me(&self) -> HumanId {
        self.me
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
    fn append(&self, event: &crate::core::Event) -> Result<(), Box<dyn std::error::Error>> {
        let mut line = crate::store::encode(event);
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
    fn load(&self) -> Result<Vec<crate::core::Event>, Box<dyn std::error::Error>> {
        let bytes = self.with_shared(read_all)?;

        let mut events = Vec::new();
        for (line, raw) in lines(complete_text(&bytes)?) {
            events.push(parse_line(raw, self.me).map_err(|source| at_line(line, source))?);
        }
        Ok(events)
    }

    /// Reads the same lines `load` does, but compares each line's id on
    /// the wire and stops at the match, so only the event asked for is
    /// ever built. A line whose payload the domain can't decode fails
    /// the fetch only when it is the line named.
    fn get(&self, id: EventId) -> Result<Option<crate::core::Event>, Box<dyn std::error::Error>> {
        let bytes = self.with_shared(read_all)?;

        for (line, raw) in lines(complete_text(&bytes)?) {
            let WireId { id: found } =
                serde_json::from_str(raw).map_err(|e| at_line(line, Error::BadLine(e)))?;
            if parse_event_id(&found).map_err(|e| at_line(line, e))? != id {
                continue;
            }
            return Ok(Some(parse_line(raw, self.me).map_err(|e| at_line(line, e))?));
        }
        Ok(None)
    }

    /// `load`'s read and `append`'s write, joined under one hold of the
    /// exclusive lock: `compute` sees exactly the events another
    /// `append_computed` call could not have raced ahead of, since
    /// nothing between this read and this write releases the lock.
    fn append_computed(
        &self,
        compute: Box<
            dyn FnOnce(Vec<crate::core::Event>) -> Result<crate::core::Event, Box<dyn std::error::Error>>
                + '_,
        >,
    ) -> Result<crate::core::Event, Box<dyn std::error::Error>> {
        self.with_exclusive(|file| {
            truncate_torn_tail(file)?;
            let bytes = read_all(file)?;
            let mut events = Vec::new();
            for (line, raw) in lines(complete_text(&bytes)?) {
                events.push(parse_line(raw, self.me).map_err(|source| at_line(line, source))?);
            }
            let event = compute(events).map_err(Error::Compute)?;
            let mut text = crate::store::encode(&event);
            text.push('\n');
            let mut file = file;
            file.write_all(text.as_bytes()).map_err(Error::Io)?;
            Ok(event)
        })
        .map_err(Into::into)
    }

    /// `append_computed`, but `compute` returns a batch: every line is
    /// built and written together, one `write_all`, before the lock
    /// this read was taken under is released.
    fn append_batch_computed(
        &self,
        compute: Box<
            dyn FnOnce(
                    Vec<crate::core::Event>,
                ) -> Result<Vec<crate::core::Event>, Box<dyn std::error::Error>>
                + '_,
        >,
    ) -> Result<Vec<crate::core::Event>, Box<dyn std::error::Error>> {
        self.with_exclusive(|file| {
            truncate_torn_tail(file)?;
            let bytes = read_all(file)?;
            let mut events = Vec::new();
            for (line, raw) in lines(complete_text(&bytes)?) {
                events.push(parse_line(raw, self.me).map_err(|source| at_line(line, source))?);
            }
            let batch = compute(events).map_err(Error::Compute)?;
            let mut text = String::new();
            for event in &batch {
                text.push_str(&crate::store::encode(event));
                text.push('\n');
            }
            let mut file = file;
            file.write_all(text.as_bytes()).map_err(Error::Io)?;
            Ok(batch)
        })
        .map_err(Into::into)
    }
}

impl EventSearch for Jsonl {
    /// Loads the whole log, then applies the query. Filtering before
    /// decode would silently skip a line with an unknown event type
    /// instead of failing loudly, which matters more here than the
    /// speed.
    fn search(
        &self,
        query: &EventQuery,
    ) -> Result<Vec<crate::core::Event>, Box<dyn std::error::Error>> {
        Ok(query.apply(self.load()?))
    }
}

/// Just enough of a line to tell whether it is the one asked for.
/// serde walks past the rest, so no payload is built for a line the
/// caller only passes over.
#[derive(Deserialize)]
struct WireId {
    id: String,
}

fn parse_line(raw: &str, me: HumanId) -> Result<crate::core::Event, Error> {
    let wire: Event = serde_json::from_str(raw).map_err(Error::BadLine)?;
    crate::store::from_wire(wire, me)
}

/// Everything up to the last newline. Bytes, not `read_to_string`: a
/// torn write can split a multi-byte character, and that tail is about
/// to be discarded.
fn complete_text(bytes: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(&bytes[..complete_len(bytes)])
        .map_err(|e| Error::Io(io::Error::new(io::ErrorKind::InvalidData, e)))
}

/// Every non-empty line, paired with its 1-based number - how the log
/// is framed, written once for both readers of it.
fn lines(contents: &str) -> impl Iterator<Item = (usize, &str)> {
    contents
        .split('\n')
        .enumerate()
        .filter(|(_, raw)| !raw.is_empty())
        .map(|(idx, raw)| (idx + 1, raw))
}

fn at_line(line: usize, source: Error) -> Error {
    Error::AtLine {
        line,
        source: Box::new(source),
    }
}

/// Reads the `HumanId` from `path`, minting a fresh UUIDv7 into it when
/// the file is missing - one open, one id. A file that exists but
/// doesn't hold a single parseable UUID is an error naming `path`,
/// never silently re-minted: overwriting it would strand every event
/// and node this human already wrote under the old one.
fn open_me(path: &Path) -> Result<HumanId, Error> {
    match fs::read_to_string(path) {
        Ok(text) => Uuid::parse_str(text.trim())
            .map(Id::from_uuid)
            .map_err(|_| Error::BadMeFile(path.to_path_buf())),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let me = HumanId::new();
            fs::write(path, format!("{}\n", me.as_uuid())).map_err(Error::Io)?;
            Ok(me)
        }
        Err(err) => Err(Error::Io(err)),
    }
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
mod tests;
