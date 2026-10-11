use crate::shared::Id;

/// One item on a map, added by a `NodeAdded` event.
pub struct Node {
    pub kind: String,
    pub title: String,
}

pub type NodeId = Id<Node>;
