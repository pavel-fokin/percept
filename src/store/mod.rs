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

/// A moment as a reader types it, on the CLI or in a tool call: ISO-8601,
/// or `<N>d`, `<N>h`, `<N>m` measured back from now. One parser, so
/// `since` means the same wherever it is written.
pub fn parse_time(s: &str) -> Result<crate::shared::Timestamp, Box<dyn std::error::Error>> {
    let parsed = match relative_minutes(s) {
        Some(minutes) => crate::shared::Timestamp::now().minus_minutes(minutes),
        None => s.parse().ok(),
    };
    parsed.ok_or_else(|| format!("invalid timestamp {s:?}: ISO-8601 or <N>d, <N>h, <N>m").into())
}

/// `<N>d`, `<N>h`, or `<N>m` as a count of minutes. `None` for anything
/// else - `parse_time` then tries it as ISO-8601.
fn relative_minutes(s: &str) -> Option<i64> {
    let (digits, unit) = s.split_at_checked(s.len().checked_sub(1)?)?;
    let n: i64 = digits.parse().ok()?;

    match unit {
        "d" => n.checked_mul(24 * 60),
        "h" => n.checked_mul(60),
        "m" => Some(n),
        _ => None,
    }
}

/// A tool's optional time bound. Absent or empty is no bound: a model
/// that fills every field the schema offers sends "" for a bound it
/// does not want, and refusing it cost a call per turn.
fn optional_time(
    s: Option<&str>,
) -> Result<Option<crate::shared::Timestamp>, Box<dyn std::error::Error>> {
    s.filter(|s| !s.is_empty()).map(parse_time).transpose()
}
