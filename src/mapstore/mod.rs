//! Folds a cognitive map from the event log and gives it an external
//! form. `LogMaps` is the `core::MapReader` the log-backed maps are
//! opened through; `fold_map` builds one and `commit` mints and
//! applies one change to it atomically, under the log's own lock;
//! `encode_*` serialize a map or a fragment to JSON lines; `markdown`,
//! `catalogue`, `describe`, and `start` render it to text for `maps
//! show`, `maps list`, `maps describe`, and `percept start`, and
//! `moment_rules` every loaded schema's lines at one of `MOMENTS`,
//! `for_schema_moment` one schema's alone - what `maps reflect` sends
//! for the one map it opened on. The
//! tools that call these live in `src/tools`. `load_schemas` reads a
//! project's schemas from `SCHEMAS_DIR` alone; `templates` is the
//! shipped TOML `percept init <client>` copies there for a project
//! that has none yet. A project with no
//! schemas has no maps: `catalogue` and `start` print `NO_SCHEMAS_HINT`
//! in place of their usual body.

// Reachability here is judged with the lab present: the lab build is
// the one that sees every consumer, and `--all-features` clippy is
// what catches code dead in both.
#![cfg_attr(not(feature = "lab"), allow(dead_code, unused_imports))]

mod blocks;
mod describe;
mod map;
pub(crate) mod outline;
mod render;
mod rules;
mod schemas;
mod start;

pub(crate) use blocks::{changed_line, gained, last_session, project_name};
pub use describe::describe;
pub use map::{
    commit, commit_batch, encode_fragment, encode_lines, encode_map, encode_schema, ensure_maps,
    fold_map, fold_map_at, of_path, paths, properties_map, start_reflection, LogMaps, NodeRefArgs,
    Snapshot,
};
pub use render::{catalogue, markdown, overview};
pub use rules::for_moment as moment_rules;
pub use rules::for_schema_moment;
pub use schemas::{
    load as load_schemas, templates, MESSAGE_RECEIVED, REFLECTION_STARTED, SCHEMAS_DIR, SESSION_STARTED,
};
pub use start::start;

/// Printed in place of `catalogue`'s and `start`'s usual body when a
/// project has declared no schema at all: the one line both renders
/// share, so a stranger sees the same instruction from either command.
pub(crate) const NO_SCHEMAS_HINT: &str =
    "no maps declared under .percept/schemas; run percept init <client>";
