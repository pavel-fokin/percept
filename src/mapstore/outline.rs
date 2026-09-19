//! The rules a map's rendered outline follows: what heads a section,
//! and what nests under what. A map is a graph, and an outline is one
//! way to print it - so these rules are `mapstore`'s, not the core's,
//! and no schema declares them. Any node may head a section; which do
//! is decided by the map's own edges, through `claimant`.

use crate::core::{EdgeEnd, Map, Node};

/// The nodes nobody claims - one `##` section each, and what a reader
/// sees of the map before opening it.
pub fn roots(map: &Map) -> Vec<&Node> {
    map.nodes().iter().filter(|node| claimant(map, node).is_none()).collect()
}

/// The node `node` prints under, if any: the first node of its own
/// kind pointing at it - the decision that supersedes it - else, of
/// the nodes of other kinds it points at, the one whose kind ranks
/// latest - the question a decision resolves over the concept it is
/// about, so rank order is nesting order and the schema's edge order
/// decides only between two of the same kind, first edge first. A node
/// nobody claims heads a section of its own. The rule names no kind,
/// so it holds for any schema: a `blocks` chore claims the one it
/// blocks the way `supersedes` claims the old decision.
pub fn claimant<'a>(map: &'a Map, node: &Node) -> Option<&'a Node> {
    let mut same_kind = None;
    let mut other_kind: Option<&Node> = None;
    for edge_kind in &map.schema().edge_kinds {
        for from in map.linked(node.id, &edge_kind.kind, EdgeEnd::To) {
            if same_kind.is_none() && from.kind == node.kind {
                same_kind = Some(from);
            }
        }
        for to in map.linked(node.id, &edge_kind.kind, EdgeEnd::From) {
            let nearer = other_kind.is_none_or(|held| rank(map, to) > rank(map, held));
            if nearer && to.kind != node.kind {
                other_kind = Some(to);
            }
        }
    }
    same_kind.or(other_kind)
}

/// A node's position by kind: its kind's index among the schema's
/// declared node kinds, so the kind declared first heads the render
/// and a node nests under the latest kind it points at.
pub fn rank(map: &Map, node: &Node) -> usize {
    map.schema()
        .node_kind_names()
        .position(|kind| kind == node.kind)
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
