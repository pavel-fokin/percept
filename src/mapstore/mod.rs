//! Folds a cognitive map from the event log and gives it an external
//! form. `LogMaps` is the `core::MapReader` the log-backed maps are
//! opened through; `fold_map` builds one and `commit` mints and
//! applies one change to it atomically, under the log's own lock;
//! `encode_*` serialize a map or a fragment to JSON lines; `markdown`
//! and `catalogue` render it to text for `maps show`/`maps list
//! --format md`. The tools that call these live in `src/tools`.

mod map;
mod render;
mod schemas;

pub use map::{
    commit, commit_batch, encode_fragment, encode_lines, encode_map, encode_schema, fold_map,
    LogMaps, NodeRefArgs, Snapshot,
};
pub use render::{catalogue, markdown};
pub use schemas::load as load_schemas;
