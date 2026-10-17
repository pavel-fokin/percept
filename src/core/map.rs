use std::collections::BTreeMap;
use std::path::PathBuf;

use super::EventId;
use crate::shared::Id;

pub type MapId = Id<Map>;
pub type NodeId = Id<Node>;

/// A named body of understanding in one project, created by a `MapCreated` event.
pub struct Map {
    pub id: MapId,
    pub kind: String,
    pub path: PathBuf,
}

/// One item on a map, added by a `NodeAdded` event.
pub struct Node {
    pub id: NodeId,
    pub kind: String,
    pub title: String,
    pub properties: BTreeMap<String, String>,
    #[allow(dead_code, reason = "reference display follows in the next issue")]
    pub references: Vec<Reference>,
    pub created_at: jiff::Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reference {
    Message { event: EventId, quote: String },
    File { path: PathBuf, lines: (u32, u32), excerpt: String },
}

/// A directed link between two nodes of a map, added by an `EdgeAdded` event.
pub struct Edge {
    pub kind: String,
    pub from: NodeId,
    pub to: NodeId,
}
