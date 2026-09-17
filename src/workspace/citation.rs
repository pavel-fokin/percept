//! Where a citation's excerpt sits in the working tree now. A stored
//! `file.cited` range says which lines were read when the citation was
//! made; it is not an address, since the text moves. `Citations`
//! answers the address question against the tree, once per path, for
//! every reader that needs it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use super::{read_text_lossy, Workspace};

/// What became of one citation: the lines its excerpt sits on now, or
/// why it was not found there.
#[derive(Debug, PartialEq, Eq)]
pub enum Cited {
    /// The 1-based inclusive line range the excerpt occupies now.
    At(u32, u32),
    /// The excerpt is gone from the cited path, but a text search of
    /// the checkout found it at exactly this one other path.
    Moved(PathBuf),
    /// The file reads, but the excerpt is no longer in it, and no
    /// other file in the checkout holds it either.
    Changed,
    /// The file is missing, binary, or outside the checkout, and no
    /// other file in the checkout holds the excerpt either.
    Gone,
}

/// The checkout a set of citations is read against, each cited path
/// read and split into lines once for this reader's life - a path more
/// than one citation names costs one read. `tree` is the same idea for
/// the fallback search a missing or changed citation runs: the working
/// tree is walked and every text file read at most once, the first
/// time any citation needs it, then reused by every later one. Built
/// per render or per request: a map is read live, so nothing here
/// outlives the call.
/// One text file in a tree walk: its path relative to the checkout
/// root, and its content split into lines.
type TreeFile = (PathBuf, Vec<String>);

pub struct Citations {
    workspace: Option<Workspace>,
    lines: RefCell<HashMap<PathBuf, Option<Vec<String>>>>,
    tree: RefCell<Option<Vec<TreeFile>>>,
}

impl Citations {
    pub fn new(checkout: &Path) -> Self {
        Self {
            workspace: Workspace::new(checkout).ok(),
            lines: RefCell::new(HashMap::new()),
            tree: RefCell::new(None),
        }
    }

    /// Where `excerpt` sits in `path` now: at its cited location, at
    /// exactly one other path a text search of the checkout found it
    /// at, or neither. The search runs only once `path` itself is
    /// gone, because the same text under a file that still exists is a
    /// copy, not a move, and naming it a rename would be a claim
    /// percept cannot support. The path comes off a log line another
    /// writer may have appended, so it becomes a real path the one way
    /// every other path does, through `Workspace`.
    pub fn locate(&self, path: &Path, excerpt: &str) -> Cited {
        if !self.lines.borrow().contains_key(path) {
            let read = self.read_file(path);
            self.lines.borrow_mut().insert(path.to_path_buf(), read);
        }
        let lines = self.lines.borrow();
        let found = lines.get(path).expect("just inserted");
        if let Some((from, to)) = found.as_ref().and_then(|lines| locate_in(excerpt, lines)) {
            return Cited::At(from, to);
        }
        let existed = found.is_some();
        drop(lines);
        if existed {
            return Cited::Changed;
        }
        match self.find_moved(path, excerpt) {
            Some(moved_to) => Cited::Moved(moved_to),
            None => Cited::Gone,
        }
    }

    fn read_file(&self, path: &Path) -> Option<Vec<String>> {
        let resolved = self.workspace.as_ref()?.resolve(&to_slash_lossy(path)).ok()?;
        let text = read_text_lossy(&resolved).ok()?;
        Some(text.lines().map(|line| line.trim_end().to_string()).collect())
    }

    /// Searches every other text file in the checkout for `excerpt`,
    /// walked once and cached for every later call. A match at exactly
    /// one path other than `path` is a move; zero or more than one is
    /// not, so the caller falls back to `Changed`/`Gone`.
    fn find_moved(&self, path: &Path, excerpt: &str) -> Option<PathBuf> {
        self.build_tree();
        let tree = self.tree.borrow();
        let tree = tree.as_ref().expect("just built");
        let mut matches = tree
            .iter()
            .filter(|(candidate, _)| candidate != path)
            .filter(|(_, lines)| locate_in(excerpt, lines).is_some());
        let first = matches.next()?.0.clone();
        matches.next().is_none().then_some(first)
    }

    fn build_tree(&self) {
        if self.tree.borrow().is_some() {
            return;
        }
        let entries = match &self.workspace {
            None => Vec::new(),
            Some(workspace) => WalkBuilder::new(workspace.root())
                .hidden(false)
                .require_git(false)
                .filter_entry(|entry| entry.file_name() != ".git")
                .build()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
                .filter_map(|entry| {
                    let text = read_text_lossy(entry.path()).ok()?;
                    let relative = PathBuf::from(workspace.relative(entry.path()));
                    let lines = text.lines().map(|line| line.trim_end().to_string()).collect();
                    Some((relative, lines))
                })
                .collect(),
        };
        *self.tree.borrow_mut() = Some(entries);
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
