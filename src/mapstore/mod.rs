//! Folds a cognitive map from the event log and gives it an external
//! form. `LogMaps` is the `core::MapReader` the log-backed maps are
//! opened through; `fold_map` builds one and `commit` mints and
//! applies one change to it atomically, under the log's own lock;
//! `encode_*` serialize a map or a fragment to JSON lines; `markdown`,
//! `catalogue`, and `describe` render it to text for `maps show`,
//! `maps list`, and `maps describe`. The
//! tools that call these live in `src/tools`. `load_schemas` reads a
//! project's schemas from `SCHEMAS_DIR` alone; `templates` is the
//! shipped TOML `percept init <client>` copies there for a project
//! that has none yet. A project with no
//! schemas has no maps: `catalogue` prints `NO_SCHEMAS_HINT` in place
//! of its usual body.

// Reachability here is judged with the lab present: the lab build is
// the one that sees every consumer, and `--all-features` clippy is
// what catches code dead in both.
#![cfg_attr(not(feature = "lab"), allow(dead_code, unused_imports))]

mod blocks;
mod describe;
mod map;
mod render;
mod schemas;

pub(crate) use blocks::{changed_line, gained, last_session, project_name};
pub use describe::describe;
pub use map::{
    commit, commit_batch, encode_fragment, encode_lines, encode_map, encode_schema, ensure_maps,
    fold_map, fold_map_at, of_path, paths, properties_map, start_reflection, LogMaps, NodeRefArgs,
    Snapshot,
};
pub use render::{catalogue, markdown};
pub use schemas::{load as load_schemas, templates, SCHEMAS_DIR};

/// Printed in place of `catalogue`'s usual body when a project has
/// declared no schema at all.
pub(crate) const NO_SCHEMAS_HINT: &str =
    "no maps declared under .percept/schemas; run percept init <client>";
