use crate::shared::Id;

pub type MapId = Id<Map>;
pub type NodeId = Id<Node>;

/// A named body of understanding in one project, created by a `MapCreated` event.
pub struct Map {
    pub id: MapId,
    pub kind: String,
    pub title: String,
}

/// One item on a map, added by a `NodeAdded` event.
pub struct Node {
    pub id: NodeId,
    pub kind: String,
    pub title: String,
}

/// A directed link between two nodes of a map, added by an `EdgeAdded` event.
pub struct Edge {
    pub kind: String,
    pub from: NodeId,
    pub to: NodeId,
}
