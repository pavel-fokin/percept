//! One turn's state on disk: the id of the turn's prompt event, so a
//! later tool call or reply in the same turn can cite it as its cause.
//! `percept hook` is the only caller today - see `cli::hook` - but the
//! file itself is infrastructure, not the hook's own concern.
//!
//! The file is also the lock: opening it takes an exclusive hold on it
//! for the life of the `TurnState`, so two hook calls for the same turn
//! never race.
//!
//! Beside the turn files, one `latest` pointer per checkout names the
//! turn open there, so a `percept maps` write from the shell, which
//! knows no session or turn id, still finds its cause.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::core::EventId;
use crate::store::{parse_event_id, Error};

/// The directory a checkout root's turns live under, so two projects
/// sharing one sessions directory never collide: `root` with every `/`
/// replaced by `%`, the one character neither path ever carries
/// itself.
pub fn turn_dir(sessions_dir: &Path, root: &Path) -> PathBuf {
    sessions_dir.join(root.to_string_lossy().replace('/', "%"))
}

pub struct TurnState {
    file: File,
    path: PathBuf,
}

impl TurnState {
    /// Opens `dir/name`, creating `dir` and the file if either is
    /// missing, and holds an exclusive lock on it until the value
    /// drops.
    pub fn open(dir: &Path, name: &str) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(name);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        file.lock()?;
        Ok(Self { file, path })
    }

    /// The id the file holds, `None` when empty.
    pub fn cause(&mut self) -> Result<Option<EventId>, Error> {
        self.file.seek(SeekFrom::Start(0)).map_err(Error::Io)?;
        let mut text = String::new();
        self.file.read_to_string(&mut text).map_err(Error::Io)?;
        parse_cause(&text)
    }

    /// Truncates the file to empty.
    pub fn clear(&mut self) -> std::io::Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        Ok(())
    }

    /// Replaces whatever the file held with `id`.
    pub fn set(&mut self, id: EventId) -> std::io::Result<()> {
        self.clear()?;
        self.file.write_all(id.as_uuid().to_string().as_bytes())
    }

    /// Unlinks the file while still locked, then drops the lock.
    pub fn remove(self) -> std::io::Result<()> {
        fs::remove_file(&self.path)
    }

    /// The cause of the turn open in `dir`'s checkout, read from the
    /// `latest` pointer `point` wrote - `None` when no turn is open.
    /// Read without a lock: a caller here is not the turn itself, and
    /// racing the hook's own write reads empty, which is the honest
    /// answer.
    pub fn latest_cause(dir: &Path) -> Result<Option<EventId>, Error> {
        match fs::read_to_string(dir.join(LATEST)) {
            Ok(text) => parse_cause(&text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::Io(err)),
        }
    }

    /// Makes `id` the cause of the turn open in `dir`'s checkout - one
    /// pointer per checkout, so a write from the shell needs no scan
    /// of the turn files, and the last prompt wins when two clients
    /// share one checkout.
    pub fn point(dir: &Path, id: EventId) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(LATEST), id.as_uuid().to_string())
    }

    /// Drops the pointer `point` wrote, so no turn is open in `dir`'s
    /// checkout. A pointer already gone is not an error.
    pub fn unpoint(dir: &Path) -> std::io::Result<()> {
        match fs::remove_file(dir.join(LATEST)) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    }
}

/// The pointer's file name within a checkout's turn directory. A turn
/// file is `<client>-<session>…`, so the two never collide.
const LATEST: &str = "latest";

/// A turn file's text as a cause: `None` when blank, else the id it
/// holds. The one rule both `cause` and `latest_cause` read through.
fn parse_cause(text: &str) -> Result<Option<EventId>, Error> {
    let text = text.trim();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parse_event_id(text)?))
    }
}

#[cfg(test)]
mod tests;
