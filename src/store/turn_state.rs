//! One turn's state on disk: the id of the turn's prompt event, so a
//! later tool call or reply in the same turn can cite it as its cause.
//! `percept hook` is the only caller today - see `cli::hook` - but the
//! file itself is infrastructure, not the hook's own concern.
//!
//! The file is also the lock: opening it takes an exclusive hold on it
//! for the life of the `TurnState`, so two hook calls for the same turn
//! never race.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::core::EventId;
use crate::store::{parse_event_id, Error};

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
        let text = text.trim();
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(parse_event_id(text)?))
        }
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
}

#[cfg(test)]
mod tests;
