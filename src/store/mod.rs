//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. `Jsonl` implements `percept::EventLog`; `SearchEvents`
//! implements `percept::Tool`, since both wire formats live here.

mod error;
mod event;
mod jsonl;
mod search_events;

pub use error::Error;
pub use event::{decode, encode, parse_actor, parse_event_id, parse_kind, summarize, Event};
pub use jsonl::Jsonl;
pub use search_events::SearchEvents;
