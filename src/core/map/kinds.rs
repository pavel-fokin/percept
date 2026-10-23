use std::collections::BTreeSet;

use super::Map;

/// The kinds a person may give a node or an edge on a map.
pub struct Kinds {
    /// Whether the map's schema declares them, so only these are allowed; else they are read off the map.
    pub declared: bool,
    pub nodes: BTreeSet<String>,
    /// The `(edge kind, from node kind, to node kind)` triples.
    pub edges: BTreeSet<(String, String, String)>,
}

impl Map {
    /// The map's declared schema when it has one, else what its nodes and edges already use.
    pub fn kinds(&self) -> Kinds {
        match &self.schema {
            Some(schema) => Kinds {
                declared: true,
                nodes: schema.nodes().keys().cloned().collect(),
                edges: schema.edges().iter().map(|(kind, edge)| (kind.clone(), edge.from.clone(), edge.to.clone())).collect(),
            },
            None => {
                let observed = self.observed();
                Kinds { declared: false, nodes: observed.nodes.into_keys().collect(), edges: observed.edges }
            }
        }
    }
}
