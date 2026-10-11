//! Domain layer: `Event` and the store trait that persists it.

mod event;
mod map;
mod node;
mod session;
mod store;

pub use event::{Actor, Event, EventId, Kind, Payload, Source};
pub use map::{Map, MapId};
pub use node::{Node, NodeId};
pub use session::{ClientSessionId, Session, SessionId};
pub use store::EventStore;
