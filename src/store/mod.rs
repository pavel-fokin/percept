//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. `Jsonl` implements `percept::EventLog`.

mod error;
mod event;
mod jsonl;

pub use error::Error;
pub use event::{actor_name, decode, encode, kind, summarize, Event};
pub use jsonl::Jsonl;
