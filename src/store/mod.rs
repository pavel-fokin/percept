//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. Only the wire format exists so far; reading and writing
//! the file comes next.

mod error;
mod event;

pub use error::Error;
#[allow(unused_imports)]
pub use event::Event;
