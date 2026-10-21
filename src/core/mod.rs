//! Domain layer: `Event` and the store traits that persist it.

mod event;
mod map;
mod name;
mod policy;
mod project;
mod schema;
mod session;
mod store;

pub use event::{Actor, Event, EventId, Kind, Payload, Source};
pub use map::{Change, Edge, Map, MapId, Node, NodeId, Reference};
pub use name::Name;
pub use policy::Policy;
pub use project::{Project, ProjectId};
pub use schema::{EdgeKind, Schema, SchemaStore};
pub use session::{ClientAgentId, ClientSessionId, Session, SessionId, SessionStatus};
pub use store::EventStore;
