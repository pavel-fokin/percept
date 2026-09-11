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
mod search;

#[cfg(test)]
pub mod testing;

pub use event::{
    cited_label, Actor, Event, EventId, EventKind, HumanId, LogCursor, LogId, Payload, Source,
    Usage, PREVIEW_CHARS,
};
pub use event_log::EventLog;
pub use map::{
    default_prefix, map_of, Edge, EdgeEnd, EdgeKind, Fragment, Map, MapError, Mutation, Node, NodeId,
    NodeKind, NodeRef, Schema, Schemas, Scope, Selection,
};
pub use map_reader::MapReader;
pub use search::{EventQuery, EventSearch};
