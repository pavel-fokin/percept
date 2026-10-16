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
    pub body: NodeBody,
    pub created_at: jiff::Timestamp,
}

/// What a node holds: a note in someone's own words, or text taken from a source.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeBody {
    Note { kind: String, title: String, properties: BTreeMap<String, String> },
    Citation(CitationSource),
}

/// Where a citation's text came from. The log keeps the text, so it stays valid when the source changes.
#[derive(Debug, Clone, PartialEq)]
pub enum CitationSource {
    Message { event: EventId, quote: String },
}

/// A directed link between two nodes of a map, added by an `EdgeAdded` event.
pub struct Edge {
    pub kind: String,
    pub from: NodeId,
    pub to: NodeId,
}
