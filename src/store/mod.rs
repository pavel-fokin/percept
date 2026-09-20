//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. `Jsonl` implements `core::EventLog` and `core::EventSearch`;
//! `event` encodes an event to a log line and back, and reads one out
//! for display. The map fold and its renderers live in `src/mapstore`;
//! the four tools the model calls over the log and its maps live in
//! `src/tools`.

// Reachability here is judged with the lab present: the lab build is
// the one that sees every consumer, and `--all-features` clippy is
// what catches code dead in both.
#![cfg_attr(not(feature = "lab"), allow(dead_code, unused_imports))]

mod error;
mod event;
mod jsonl;
mod turn_state;

pub use error::Error;
pub use event::{
    encode, encode_at, find_event, from_wire, ids, parse_actor, wire_actor, WireActor,
    parse_event_id, parse_kind, parse_lines, parse_map_id, read_event, summarize, summary, Cursor,
    Event, PREVIEW_CHARS,
};
pub use jsonl::Jsonl;
pub use turn_state::{turn_dir, TurnState};

/// A moment in a tool call, refused in the vocabulary a tool schema uses.
/// The grammar is `shared::parse_time`'s; only the wording is here.
fn parse_time(s: &str) -> Result<crate::shared::Timestamp, Box<dyn std::error::Error>> {
    crate::shared::parse_time(s)
        .ok_or_else(|| format!("invalid timestamp {s:?}: ISO-8601 or <N>d, <N>h, <N>m").into())
}

/// A tool's optional time bound. Absent or empty is no bound: a model
/// that fills every field the schema offers sends "" for a bound it
/// does not want, and refusing it cost a call per turn.
pub fn optional_time(
    s: Option<&str>,
) -> Result<Option<crate::shared::Timestamp>, Box<dyn std::error::Error>> {
    s.filter(|s| !s.is_empty()).map(parse_time).transpose()
}
