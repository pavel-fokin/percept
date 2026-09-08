//! Folds a cognitive map from the event log and gives it an external
//! form. `LogMaps` is the `core::MapReader` the log-backed maps are
//! opened through; `fold_map` and `revise` build and change one;
//! `encode_*` serialize a map or a fragment to JSON lines; `markdown`
//! and `MarkdownFiles` render it to `.percept/` as Markdown. The tools
//! that call these live in `src/tools`.

mod map;
mod render;

pub use map::{
    encode_fragment, encode_lines, encode_map, encode_schema, fold_map, revise, LogMaps,
    NodeRefArgs, Snapshot,
};
pub use render::{catalogue, markdown, MarkdownFiles};
