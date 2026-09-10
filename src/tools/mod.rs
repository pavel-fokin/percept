//! Every tool the model calls. `search_events`, `read_event`,
//! `revise_map`, and `read_map` run over the event log and its maps,
//! through `store` and `mapstore`. `read_file`, `list_files`,
//! `find_files`, `grep_files`, `write_file`, `edit_file`, `bash`, and
//! `read_code` run over a working tree; `Workspace` is the one place a
//! path the model gave becomes a real path, shared by every file tool
//! and by the CLI's citations, so it and the text reader build without
//! the `lab` feature the tools need.
//! `read_code` walks the tree fresh through `code::build` on every
//! call. Beside them, what a turn over the tree needs:
//! `AskBeforeWrites`, the `Policy` that puts a write or a command to
//! the user, and `GitSnapshot`, the `Snapshot` that saves the tree
//! before each prompt.

#[cfg(feature = "lab")]
mod bash;
#[cfg(feature = "lab")]
mod edit_file;
#[cfg(feature = "lab")]
mod find_files;
#[cfg(feature = "lab")]
mod git_snapshot;
#[cfg(feature = "lab")]
mod grep_files;
#[cfg(feature = "lab")]
mod list_files;
#[cfg(feature = "lab")]
mod policy;
#[cfg(feature = "lab")]
mod read_code;
#[cfg(feature = "lab")]
mod read_event;
#[cfg(feature = "lab")]
mod read_file;
#[cfg(feature = "lab")]
mod read_map;
#[cfg(feature = "lab")]
mod revise_map;
#[cfg(feature = "lab")]
mod search_events;
mod workspace;
#[cfg(feature = "lab")]
mod write_file;

#[cfg(feature = "lab")]
pub use bash::Bash;
#[cfg(feature = "lab")]
pub use edit_file::EditFile;
#[cfg(feature = "lab")]
pub use find_files::FindFiles;
#[cfg(feature = "lab")]
pub use git_snapshot::GitSnapshot;
#[cfg(feature = "lab")]
pub use grep_files::GrepFiles;
#[cfg(feature = "lab")]
pub use list_files::ListFiles;
#[cfg(feature = "lab")]
pub use policy::AskBeforeWrites;
#[cfg(feature = "lab")]
pub use read_code::ReadCode;
#[cfg(feature = "lab")]
pub use read_event::ReadEvent;
#[cfg(feature = "lab")]
pub use read_file::ReadFile;
#[cfg(feature = "lab")]
pub use read_map::ReadMap;
#[cfg(feature = "lab")]
pub use revise_map::ReviseMap;
#[cfg(feature = "lab")]
pub use search_events::SearchEvents;
pub use workspace::Workspace;
#[cfg(feature = "lab")]
pub use write_file::WriteFile;

use std::path::Path;

#[cfg(feature = "lab")]
use crate::core::{Map, MapError, NodeRef, Selection};
#[cfg(feature = "lab")]
use crate::harness::ToolOutput;
#[cfg(feature = "lab")]
use crate::mapstore::{encode_fragment, encode_lines, encode_schema, NodeRefArgs};
#[cfg(feature = "lab")]
use crate::shared::Timestamp;

/// How much of a file's start is checked for a NUL byte before it is
/// treated as text.
const BINARY_SNIFF_BYTES: usize = 8192;

/// Whether `bytes` are a binary file: a NUL in the first 8 KiB. What
/// `read_file`, `edit_file`, `grep_files`, and `read_text_lossy` all
/// refuse or skip.
fn is_binary(bytes: &[u8]) -> bool {
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

/// What `read_map` and `read_code` share past building their own `Map`:
/// resolves `around` against it, cuts it to the `Selection` `around`,
/// `depth`, `since`, and `kinds` make, and encodes the schema, the
/// fragment's counts, and its lines. `stamped` is `false` for
/// `read_code`'s tree walk, whose nodes carry no actor or time to show;
/// `true` for a map folded from the log.
#[cfg(feature = "lab")]
pub(crate) fn read_selection(
    map: Map,
    around: Option<NodeRefArgs>,
    depth: usize,
    since: Option<Timestamp>,
    kinds: &[String],
    stamped: bool,
) -> Result<ToolOutput, Box<dyn std::error::Error>> {
    // An empty map has nothing to resolve `around` against - `select`'s
    // own empty-map case would skip it anyway, so a node named on one is
    // not an error to report over "nothing found yet".
    let around = if map.nodes().is_empty() {
        None
    } else {
        around
            .map(|node| -> Result<NodeRef, MapError> {
                let id = node.resolve(&map)?;
                let node = map.node(id).expect("resolve returns a live node's id");
                Ok(NodeRef {
                    kind: node.kind.clone(),
                    name: node.name.clone(),
                })
            })
            .transpose()?
    };
    let selection = Selection {
        around: around.as_ref().map(|node| (node, depth)),
        since,
        kinds,
    };
    let fragment = map.select(&selection)?;
    let lines = [
        encode_schema(fragment.map().schema()),
        encode_fragment(&fragment),
    ]
    .into_iter()
    .chain(encode_lines(fragment.map(), stamped));
    Ok(ToolOutput::text(lines.collect::<Vec<_>>().join("\n")))
}

/// `lines` joined, at most `cap` of them, with `trailer` on its own
/// last line when any were cut - told how many, so a tool can say what
/// to do about it.
#[cfg(feature = "lab")]
fn join_capped(
    mut lines: Vec<String>,
    cap: usize,
    trailer: impl FnOnce(usize) -> String,
) -> String {
    let remaining = lines.len().saturating_sub(cap);
    lines.truncate(cap);
    if remaining > 0 {
        lines.push(trailer(remaining));
    }
    lines.join("\n")
}
