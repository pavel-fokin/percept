use std::collections::{BTreeMap, BTreeSet};

use super::Map;

/// The kinds a map uses, read off its nodes and edges. Unlike a `Schema`, it checks nothing,
/// and one edge kind may link several pairs of node kinds.
pub struct Observed {
    /// Each node kind with the union of property names its nodes hold.
    pub nodes: BTreeMap<String, BTreeSet<String>>,
    /// The `(edge kind, from node kind, to node kind)` triples.
    pub edges: BTreeSet<(String, String, String)>,
}

impl Map {
    pub fn observed(&self) -> Observed {
        let mut nodes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for node in &self.nodes {
            nodes.entry(node.kind.clone()).or_default().extend(node.properties.keys().cloned());
        }

        let edges = self
            .edges
            .iter()
            .filter_map(|e| Some((e.kind.clone(), self.node(e.from)?.kind.clone(), self.node(e.to)?.kind.clone())))
            .collect();

        Observed { nodes, edges }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Actor, Event, MapId, NodeId, Source};

    fn node(map: MapId, id: NodeId, kind: &str, title: &str, keys: &[&str]) -> Event {
        let properties = keys.iter().map(|k| (k.to_string(), "v".to_string())).collect();

        Event::node_added(Source::fixture(), Actor::Human, map, id, kind, title, properties, Vec::new())
    }

    #[test]
    fn properties_are_the_union_over_the_nodes_of_a_kind() {
        let (map, a, b, c) = (MapId::new(), NodeId::new(), NodeId::new(), NodeId::new());
        let events = [
            node(map, a, "obstacle", "A", &["where"]),
            node(map, b, "obstacle", "B", &["when", "where"]),
            node(map, c, "cause", "C", &[]),
        ];

        let observed = Map::fixture(map, &events).observed();

        assert_eq!(observed.nodes.len(), 2);
        assert_eq!(observed.nodes["obstacle"], BTreeSet::from(["when".to_string(), "where".to_string()]));
        assert!(observed.nodes["cause"].is_empty());
        assert!(observed.edges.is_empty());
    }

    #[test]
    fn an_edge_kind_between_two_pairs_of_kinds_gives_two_triples() {
        let (map, a, b, c) = (MapId::new(), NodeId::new(), NodeId::new(), NodeId::new());
        let events = [
            node(map, a, "obstacle", "A", &[]),
            node(map, b, "cause", "B", &[]),
            node(map, c, "obstacle", "C", &[]),
            Event::edge_added(Source::fixture(), Actor::Human, map, "caused-by", a, b),
            Event::edge_added(Source::fixture(), Actor::Human, map, "caused-by", c, b),
            Event::edge_added(Source::fixture(), Actor::Human, map, "caused-by", b, a),
        ];

        let observed = Map::fixture(map, &events).observed();

        let triple = |k: &str, f: &str, t: &str| (k.to_string(), f.to_string(), t.to_string());
        assert_eq!(observed.edges, BTreeSet::from([triple("caused-by", "cause", "obstacle"), triple("caused-by", "obstacle", "cause")]));
    }
}
