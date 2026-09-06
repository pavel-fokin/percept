//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. `Jsonl` implements `percept::EventLog`; `SearchEvents`,
//! `ReadEvent`, `ReviseMap`, and `ReadMap` implement `percept::Tool`,
//! since all four wire formats live here. `map` folds a cognitive map
//! from the log and prints it; `render`'s `MarkdownFiles` implements
//! `percept::MapRenderer`, writing a map's fold to `.percept/` as
//! Markdown on every write.

mod error;
mod event;
mod jsonl;
mod map;
mod read_event;
mod read_map;
mod render;
mod revise_map;
mod search_events;

pub use error::Error;
pub use event::{
    decode, encode, excerpt, parse_actor, parse_event_id, parse_kind, summarize, Event,
    PREVIEW_CHARS,
};
pub use jsonl::Jsonl;
pub use map::{encode_fragment, encode_lines, encode_map, fold_map, revise, Snapshot};
pub use read_event::{read, ReadEvent};
pub use read_map::ReadMap;
pub use render::MarkdownFiles;
pub use revise_map::ReviseMap;
pub use search_events::SearchEvents;

/// A tool's optional time bound. Absent or empty is no bound: a model
/// that fills every field the schema offers sends "" for a bound it
/// does not want, and refusing it cost a call per turn. Otherwise
/// ISO-8601 only - the model is told the current time and works out
/// absolute bounds itself, so no relative shorthand and no clock here.
fn optional_time(
    s: Option<&str>,
) -> Result<Option<crate::shared::Timestamp>, Box<dyn std::error::Error>> {
    s.filter(|s| !s.is_empty())
        .map(|s| {
            s.parse()
                .map_err(|_| format!("invalid timestamp {s:?}, expected ISO-8601").into())
        })
        .transpose()
}
