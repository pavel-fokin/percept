//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. `Jsonl` implements `percept::EventLog`; `SearchEvents`
//! and `ReadEvent` implement `percept::Tool`, since both wire formats
//! live here.

mod error;
mod event;
mod jsonl;
mod read_event;
mod search_events;

pub use error::Error;
pub use event::{
    decode, encode, excerpt, parse_actor, parse_event_id, parse_kind, summarize, Event,
    PREVIEW_CHARS,
};
pub use jsonl::Jsonl;
pub use read_event::ReadEvent;
pub use search_events::SearchEvents;
