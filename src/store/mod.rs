//! The JSONL event log and its serde boundary - the domain stays
//! serde-free. `Jsonl` reads and writes the file; wiring it into the
//! app comes next.

mod error;
mod event;
mod jsonl;

pub use error::Error;
#[allow(unused_imports)]
pub use event::Event;
#[allow(unused_imports)]
pub use jsonl::Jsonl;
