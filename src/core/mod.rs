//! Domain layer: `Event` and the store traits that persist it.

mod board;
mod event;
mod map;
mod name;
mod project;
mod session;
mod store;

pub use board::{Board, BoardId};
pub use event::{Actor, Event, EventId, Kind, Payload, Source};
pub use map::{Definition, DefinitionStore, Edge, EdgeKind, Map, MapId, Node, NodeId, LastPlace, Place, Places, Policy, Reference, Schema};
pub use name::Name;
pub use project::{Project, ProjectId};
pub use session::{ClientAgentId, ClientSessionId, Session, SessionId, SessionStatus};
pub use store::EventStore;
