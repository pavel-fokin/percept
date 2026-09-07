//! The working tree a file tool reads from, and the one place a path
//! the model gave becomes a real path on disk.

use std::collections::HashSet;
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

        let mut existing = normalized.as_path();
        let mut rest = PathBuf::new();
        loop {
            if existing.exists() {
                break;
            }
            let Some(parent) = existing.parent() else {
                break;
            };
            let Some(name) = existing.file_name() else {
                break;
            };
            rest = PathBuf::from(name).join(rest);
            existing = parent;
        }

        let canonical_existing = existing
            .canonicalize()
            .unwrap_or_else(|_| existing.to_path_buf());
        let resolved = if rest.as_os_str().is_empty() {
            canonical_existing
        } else {
            canonical_existing.join(rest)
        };

        if !resolved.starts_with(&self.root) {
            return Err(format!("{path} is outside the workspace").into());
        }
        Ok(resolved)
    }

    /// The path as the model should see it: relative to the root,
    /// `/`-separated regardless of platform.
    pub fn relative(&self, path: &Path) -> String {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
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

#[cfg(test)]
mod tests;
