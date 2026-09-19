//! The working tree a file tool reads from, and the one place a path
//! the model gave becomes a real path on disk.

// Reachability here is judged with the lab present: the lab build is
// the one that sees every consumer, and `--all-features` clippy is
// what catches code dead in both.
#![cfg_attr(not(feature = "lab"), allow(dead_code, unused_imports))]

use std::collections::HashSet;
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::shared::to_slash;

mod citation;
pub use citation::{Cited, Citations};

/// A working tree rooted at an absolute, canonical path, plus the set
/// of files a tool has actually read - the record a later edit tool
/// checks before writing, so an edit of a file the model has not read
/// stays an edit from memory, never allowed.
pub struct Workspace {
    root: PathBuf,
    read: Mutex<HashSet<PathBuf>>,
}

impl Workspace {
    /// Canonicalises `root` so every resolved path can be compared
    /// against it with a plain prefix check.
    pub fn new(root: &Path) -> io::Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
            read: Mutex::new(HashSet::new()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Turns a path the model gave into an absolute path inside the
    /// workspace, refusing anything that resolves outside it - `..`
    /// climbing past the root, an absolute path elsewhere, or a
    /// symlink whose target escapes. Lexical `.`/`..` normalisation
    /// runs first, then the longest existing prefix of the result is
    /// canonicalised - resolving any symlink in it - and the rest is
    /// reattached, so a file that does not exist yet still resolves as
    /// long as its parent does.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, Box<dyn Error>> {
        let candidate = Path::new(path);
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.join(candidate)
        };

        let normalized = normalize(&joined);

        // `symlink_metadata`, not `exists`: a dangling symlink exists as
        // a link, and must be resolved as one - `exists` would follow
        // it, call it missing, and let a write create its target.
        let mut existing = normalized.as_path();
        let mut missing = Vec::new();
        while existing.symlink_metadata().is_err() {
            let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
                break;
            };
            missing.push(name.to_os_string());
            existing = parent;
        }

        let mut resolved = existing
            .canonicalize()
            .map_err(|err| format!("{path} cannot be resolved: {err}"))?;
        for name in missing.into_iter().rev() {
            resolved.push(name);
        }

        if !resolved.starts_with(&self.root) {
            return Err(format!("{path} is outside the workspace").into());
        }
        Ok(resolved)
    }

    /// The path as the model should see it: relative to the root,
    /// `/`-separated.
    pub fn relative(&self, path: &Path) -> String {
        to_slash(path.strip_prefix(&self.root).unwrap_or(path))
    }

    /// Records that `read_file` has returned `path`'s contents.
    pub fn mark_read(&self, path: &Path) {
        self.read.lock().unwrap().insert(path.to_path_buf());
    }

    /// Whether `path` has been read this session.
    pub fn was_read(&self, path: &Path) -> bool {
        self.read.lock().unwrap().contains(path)
    }
}

/// Resolves `.` and `..` components lexically, without touching the
/// filesystem - the symlink check happens afterwards, against whatever
/// prefix of the result actually exists.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// How much of a file's start is checked for a NUL byte before it is
/// treated as text.
const BINARY_SNIFF_BYTES: usize = 8192;

/// Whether `bytes` are a binary file: a NUL in the first 8 KiB. What
/// `read_file`, `edit_file`, `grep_files`, and `read_text_lossy` all
/// refuse or skip.
pub(crate) fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
}

/// Reads `path` as text, refusing a binary file - a NUL in the first
/// 8 KiB - the same rule `read_file` reads by. Valid UTF-8 is kept as
/// is; anything else is decoded lossily, so one invalid byte in an
/// otherwise-text file doesn't fail the read. Shared by `cli::publish`'s
/// `file.cited` payload and `cli::hook`'s `changed since recorded`
/// check, so the two sides of "does this citation still read" agree on
/// what counts as text.
pub(crate) fn read_text_lossy(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    if is_binary(&bytes) {
        return Err(format!("{} is binary", path.display()).into());
    }
    Ok(match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    })
}

/// A workspace over a fresh temp dir, for every tool's tests. The dir
/// is returned too, since dropping it removes the tree.
#[cfg(test)]
pub fn temp_workspace() -> (tempfile::TempDir, std::sync::Arc<Workspace>) {
    let dir = tempfile::tempdir().unwrap();
    let workspace = std::sync::Arc::new(Workspace::new(dir.path()).unwrap());
    (dir, workspace)
}

#[cfg(test)]
mod tests;
