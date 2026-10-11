//! Domain layer: `Event` and the store trait that persists it.

mod event;
mod session;
mod store;

pub use event::{Actor, Event, EventId, Payload};
pub use session::{ClientSessionId, Session, SessionId};
pub use store::EventStore;
