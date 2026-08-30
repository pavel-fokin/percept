//! JSON wire format for the event log. The serde boundary - the domain
//! stays serde-free. Consumed once the persistence layer lands; until
//! then only the round-trip tests exercise it.
#![allow(dead_code, unused_imports)]

mod event;

pub use event::{EventDto, FromDtoError};
