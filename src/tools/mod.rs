//! Every tool the model calls. `search_events`, `read_event`,
//! `revise_map`, and `read_map` run over the event log and its maps,
//! through `store` and `mapstore`. `read_file`, `list_files`,
//! `find_files`, `grep_files`, `write_file`, `edit_file`, `bash`, and
//! `read_code` run over a working tree; `Workspace` is the one place a
//! path the model gave becomes a real path, shared by every file tool.
//! `read_code` walks the tree fresh through `code::build` on every
//! call. Beside them, what a turn over the tree needs:
//! `AskBeforeWrites`, the `Policy` that puts a write or a command to
//! the user, and `GitSnapshot`, the `Snapshot` that saves the tree
//! before each prompt.

mod bash;
mod edit_file;
mod find_files;
mod git_snapshot;
mod grep_files;
mod list_files;
mod policy;
mod read_code;
mod read_event;
mod read_file;
mod read_map;
mod revise_map;
mod search_events;
mod workspace;
mod write_file;

pub use bash::Bash;
pub use edit_file::EditFile;
pub use find_files::FindFiles;
pub use git_snapshot::GitSnapshot;
pub use grep_files::GrepFiles;
pub use list_files::ListFiles;
pub use policy::AskBeforeWrites;
pub use read_code::ReadCode;
pub use read_event::{read, ReadEvent};
pub use read_file::ReadFile;
pub use read_map::ReadMap;
pub use revise_map::ReviseMap;
pub use search_events::SearchEvents;
pub use workspace::Workspace;
pub use write_file::WriteFile;

use std::path::Path;

use crate::core::{Map, MapError, NodeRef, Selection};
use crate::harness::ToolOutput;
use crate::mapstore::{encode_fragment, encode_lines, encode_schema, NodeRefArgs};
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
