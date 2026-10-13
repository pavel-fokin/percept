//! Domain layer: `Event` and the store trait that persists it.

mod event;
mod map;
mod session;
mod store;

pub use event::{Actor, Event, EventId, Kind, Payload, Source};
pub use map::{Edge, Map, MapId, Node, NodeId};
pub use session::{ClientAgentId, ClientSessionId, Session, SessionId};
pub use store::EventStore;
