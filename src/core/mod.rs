//! Domain layer: `Event` and the store traits that persist it.

mod event;
mod map;
mod schema;
mod session;
mod store;

pub use event::{Actor, Event, EventId, Kind, Payload, Source};
pub use map::{Edge, Map, MapId, Node, NodeId};
pub use schema::{EdgeKind, Schema, SchemaStore};
pub use session::{ClientAgentId, ClientSessionId, Session, SessionId};
pub use store::EventStore;
