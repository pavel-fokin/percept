use crate::shared::Id;

/// One item on a map, added by a `NodeAdded` event.
pub struct Node {
    pub id: NodeId,
    pub kind: String,
    pub title: String,
}

pub type NodeId = Id<Node>;

/// A directed link between two nodes of a map, added by an `EdgeAdded` event.
pub struct Edge {
    pub kind: String,
    pub from: NodeId,
    pub to: NodeId,
}
