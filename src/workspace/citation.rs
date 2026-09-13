//! Where a citation's excerpt sits in the working tree now. A stored
//! `file.cited` range says which lines were read when the citation was
//! made; it is not an address, since the text moves. `Citations`
//! answers the address question against the tree, once per path, for
//! every reader that needs it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{read_text_lossy, Workspace};

/// What became of one citation: the lines its excerpt sits on now, or
/// why it was not found there.
#[derive(Debug, PartialEq, Eq)]
pub enum Cited {
    /// The 1-based inclusive line range the excerpt occupies now.
    At(u32, u32),
    /// The file reads, but the excerpt is no longer in it.
    Changed,
    /// The file is missing, binary, or outside the checkout.
    Gone,
}

/// The checkout a set of citations is read against, each cited path
/// read and split into lines once for this reader's life - a path more
/// than one citation names costs one read. Built per render or per
/// request: a map is read live, so nothing here outlives the call.
pub struct Citations {
    workspace: Option<Workspace>,
    lines: RefCell<HashMap<PathBuf, Option<Vec<String>>>>,
}

impl Citations {
    pub fn new(checkout: &Path) -> Self {
        Self {
            workspace: Workspace::new(checkout).ok(),
            lines: RefCell::new(HashMap::new()),
        }
    }

    /// Where `excerpt` sits in `path` now. The path comes off a log
    /// line another writer may have appended, so it becomes a real
    /// path the one way every other path does, through `Workspace`.
    pub fn locate(&self, path: &Path, excerpt: &str) -> Cited {
        if !self.lines.borrow().contains_key(path) {
            let read = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.resolve(&to_slash_lossy(path)).ok())
                .and_then(|resolved| read_text_lossy(&resolved).ok())
                .map(|text| text.lines().map(|line| line.trim_end().to_string()).collect());
            self.lines.borrow_mut().insert(path.to_path_buf(), read);
        }
        match self.lines.borrow().get(path).expect("just inserted") {
            None => Cited::Gone,
            Some(lines) => match locate_in(excerpt, lines) {
                Some((from, to)) => Cited::At(from, to),
                None => Cited::Changed,
            },
        }
    }
}

fn to_slash_lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The excerpt's own lines: each one's trailing whitespace stripped,
/// then leading and trailing blank lines dropped, so a citation whose
/// stored excerpt padded its range with context still matches.
fn excerpt_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().map(|line| line.trim_end()).collect();
    let start = lines.iter().position(|line| !line.is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|line| !line.is_empty()).map_or(start, |i| i + 1);
    lines[start..end].to_vec()
}

/// The 1-based inclusive line range where `excerpt` sits in `lines`,
/// or `None` when it does not: the excerpt's lines are matched whole
/// against a contiguous run, first match wins. `lines` keeps its own
/// leading blanks, unlike the excerpt's: dropping them would shift
/// every number. An excerpt of nothing but blanks is never found.
pub fn locate_in(excerpt: &str, lines: &[String]) -> Option<(u32, u32)> {
    let excerpt = excerpt_lines(excerpt);
    if excerpt.is_empty() || excerpt.len() > lines.len() {
        return None;
    }
    (0..=lines.len() - excerpt.len())
        .find(|&start| lines[start..start + excerpt.len()] == excerpt[..])
        .map(|start| (start as u32 + 1, (start + excerpt.len()) as u32))
}

#[cfg(test)]
mod tests;
