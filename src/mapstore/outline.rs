//! What heads a map's outline. A map is a forest: every edge runs from
//! a node to one hanging under it, and `Map::apply` refuses a second
//! edge into one node or one that closes a cycle. So there is no rule
//! to apply here beyond reading the shape - the roots are the nodes no
//! edge reaches, and the render walks down from them.

use crate::core::{Map, Node};

/// The nodes no edge reaches - one `##` section each, and what a
/// reader sees of the map before opening it. Never empty for a map
/// that holds a node, since a forest has no cycle to swallow one.
pub fn roots(map: &Map) -> Vec<&Node> {
    map.nodes().iter().filter(|node| map.parent(node.id).is_none()).collect()
}

#[cfg(test)]
mod tests;
