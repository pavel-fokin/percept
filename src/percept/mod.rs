mod event;
mod id;
mod message;

// EventId and Id aren't referenced outside this module yet - Event's own
// field uses EventId internally, and Id has no second entity to serve
// yet - but both are part of the module's public shape per the ADR, so
// the re-exports stay and the lint is suppressed rather than removed.
#[allow(unused_imports)]
pub use event::{Event, EventId, Sender};
#[allow(unused_imports)]
pub use id::Id;
// Role isn't referenced outside this module yet - Stub ignores messages
// entirely while it streams static text - but it's part of Message's
// public shape, so the re-export stays and the lint is suppressed
// rather than removed.
#[allow(unused_imports)]
pub use message::{to_messages, Message, Model, Role};
