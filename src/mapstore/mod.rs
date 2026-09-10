//! Folds a cognitive map from the event log and gives it an external
//! form. `LogMaps` is the `core::MapReader` the log-backed maps are
//! opened through; `fold_map` builds one and `commit` mints and
//! applies one change to it atomically, under the log's own lock;
//! `encode_*` serialize a map or a fragment to JSON lines; `markdown`
//! and `catalogue` render it to text for `maps show`/`maps list
//! --format md`. The tools that call these live in `src/tools`.

// Reachability here is judged with the lab present: the lab build is
// the one that sees every consumer, and `--all-features` clippy is
// what catches code dead in both.
#![cfg_attr(not(feature = "lab"), allow(dead_code, unused_imports))]

mod blocks;
mod judge;
mod map;
mod render;
mod schemas;

pub(crate) use blocks::{
    block_header, capped_lines, judged_since_block, latest_session_per_client, line_id,
};
pub(crate) use judge::{confirm, dispute, resolve_judged_node, JudgeError};
pub use map::{
    commit, commit_batch, encode_fragment, encode_lines, encode_map, encode_schema, fold_map,
    LogMaps, NodeRefArgs, Snapshot,
};
pub use render::{catalogue, markdown};
pub use schemas::load as load_schemas;
