//! Folds a cognitive map from the event log and gives it an external
//! form. `LogMaps` is the `core::MapReader` the log-backed maps are
//! opened through; `fold_map` builds one and `commit` mints and
//! applies one change to it atomically, under the log's own lock;
//! `encode_*` serialize a map or a fragment to JSON lines; `markdown`
//! and `catalogue` render it to text for `maps show`/`maps list`. The
//! tools that call these live in `src/tools`.

// Reachability here is judged with the lab present: the lab build is
// the one that sees every consumer, and `--all-features` clippy is
// what catches code dead in both.
#![cfg_attr(not(feature = "lab"), allow(dead_code, unused_imports))]

mod blocks;
mod describe;
mod map;
mod render;
mod schemas;

pub(crate) use blocks::{block_header, capped_lines, changed_line, last_session, last_session_at, line_id, LIMIT};
pub use describe::describe;
pub use map::{
    commit, commit_batch, encode_fragment, encode_lines, encode_map, encode_schema, fold_map, fold_map_at,
    of_path, paths, LogMaps, NodeRefArgs, Snapshot,
};
pub use render::{catalogue, markdown};
pub use schemas::load as load_schemas;
