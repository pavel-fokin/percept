mod event;
mod event_log;
mod message;

// EventId isn't referenced outside this module yet - Event's own field
// uses it internally - but it's part of the module's public shape per
// the ADR, so the re-export stays and the lint is suppressed.
#[allow(unused_imports)]
pub use event::{Actor, Event, EventId, Payload};
pub use event_log::EventLog;
pub use message::{to_messages, Message, Model};
