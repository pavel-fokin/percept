//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. `Jsonl` implements `percept::EventLog`; `SearchEvents`,
//! `ReadEvent`, and `ReviseMap` implement `percept::Tool`, since all
//! three wire formats live here. `map` folds a cognitive map from the
//! log and prints it.

mod error;
mod event;
mod jsonl;
mod map;
mod read_event;
mod revise_map;
mod search_events;

pub use error::Error;
pub use event::{
    decode, encode, excerpt, parse_actor, parse_event_id, parse_kind, summarize, Event,
    PREVIEW_CHARS,
};
pub use jsonl::Jsonl;
pub use map::{encode_edge, encode_map, encode_node, fold_map, revise, Snapshot};
pub use read_event::{read, ReadEvent};
pub use revise_map::ReviseMap;
pub use search_events::SearchEvents;
