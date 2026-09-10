//! percept's core: an append-only experience log and the cognitive
//! maps folded from it, with the rules between them. Serde-free; knows
//! nothing of a model, a tool, or a working tree.

mod event;
mod event_log;
mod map;
mod map_reader;
mod search;

#[cfg(test)]
pub mod testing;

pub use event::{
    cited_label, Actor, Event, EventId, EventKind, Payload, Source, Usage, PREVIEW_CHARS,
};
pub use event_log::EventLog;
#[cfg(test)]
pub use map::{REOPENS, SUPERSEDES};
pub use map::{
    default_prefix, map_of, Edge, Fragment, Kind, Map, MapError, Mutation, Node, NodeId, NodeRef,
    Schema, Schemas, Scope, Selection, Settlement, DECISION,
};
pub use map_reader::MapReader;
pub use search::{EventQuery, EventSearch};
