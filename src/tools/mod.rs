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

/// How much of a file's start is checked for a NUL byte before it is
/// treated as text.
const BINARY_SNIFF_BYTES: usize = 8192;

/// Whether `bytes` are a binary file: a NUL in the first 8 KiB. What
/// `read_file`, `edit_file` and `grep_files` all refuse or skip.
pub(crate) fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
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
