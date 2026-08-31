//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. `Jsonl` implements `percept::EventLog`.

mod error;
mod event;
mod jsonl;

pub use error::Error;
pub use event::{decode, encode, kind, parse_actor, parse_event_id, summarize, Event, KINDS};
pub use jsonl::Jsonl;
