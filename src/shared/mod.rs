//! Foundational value types shared across layers, below the domain.
//! No domain meaning of their own.

mod id;
mod time;

pub use id::Id;
pub use time::Timestamp;
