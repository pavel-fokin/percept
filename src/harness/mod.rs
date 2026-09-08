//! percept's harness ports: what a loop needs to drive a model over
//! the core - a `Model` to reply, `Tool`s it can call, a `Policy` that
//! gates a call, a `Snapshot` that saves the tree. Depends on `core`,
//! never the other way.

mod model;
mod policy;
mod snapshot;
mod tool;

#[cfg(test)]
pub mod testing;

pub use model::{
    message_of, Chunk, Message, Modality, Model, ModelCapabilities, ModelCatalog, ModelDescriptor,
    ModelListing, ModelRequest, Provider, ReasoningEffort, ReplyStream,
};
pub use policy::{AllowAll, Policy, Verdict};
pub use snapshot::Snapshot;
pub use tool::{Tool, ToolOutput, ToolSpec};
