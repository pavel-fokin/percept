//! percept's core: an append-only experience log and the cognitive
//! maps folded from it, with the rules between them. Serde-free; knows
//! nothing of a model, a tool, or a working tree.

// Reachability here is judged with the lab present: the lab build is
// the one that sees every consumer, and `--all-features` clippy is
// what catches code dead in both.
#![cfg_attr(not(feature = "lab"), allow(dead_code, unused_imports))]

mod event;
mod event_log;
mod map;
mod map_reader;
mod schema;
mod search;

#[cfg(test)]
pub mod testing;

pub use event::{
    cited_label, Actor, Event, EventId, EventKind, HumanId, LogCursor, LogId, Payload, Source,
    Usage, PREVIEW_CHARS,
};
pub(crate) use event_log::ComputeEvents;
pub use event_log::EventLog;
pub use map::{
    map_id_for, map_of, map_of_mut, Change, Edge, Fragment, Map, MapError, MapId, Mutation, Node,
    NodeId, NodeRef, Selection, Written,
};
pub use map_reader::MapReader;
#[allow(unused_imports)]
pub use schema::{check_across, fold_all, fold_named, map_created_at, map_id_at, map_root, EdgeKind, NodeKind, Schema, SchemaError, Schemas};
pub use search::{EventQuery, EventSearch};
