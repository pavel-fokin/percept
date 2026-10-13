use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::shared::Result;

/// The shape of one kind of map: which nodes and edges it allows.
#[allow(dead_code, reason = "read by node and edge validation, the next issue")]
pub struct Schema {
    purpose: String,
    instructions: String,
    nodes: BTreeMap<String, BTreeSet<String>>,
    edges: BTreeMap<String, EdgeKind>,
}

/// The node kinds an edge kind links.
pub struct EdgeKind {
    pub from: String,
    pub to: String,
}

#[allow(dead_code, reason = "read by node and edge validation, the next issue")]
impl Schema {
    /// A schema needs a node kind, and each edge kind links declared node kinds.
    pub fn new(
        purpose: String,
        instructions: String,
        nodes: BTreeMap<String, BTreeSet<String>>,
        edges: BTreeMap<String, EdgeKind>,
    ) -> Result<Self> {
        if nodes.is_empty() {
            return Err("a schema needs at least one node kind".into());
        }

        for (kind, edge) in &edges {
            for end in [&edge.from, &edge.to] {
                if !nodes.contains_key(end) {
                    return Err(format!("edge {kind:?} names the undeclared node kind {end:?}").into());
                }
            }
        }

        Ok(Self { purpose, instructions, nodes, edges })
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    /// Each node kind with its property names.
    pub fn nodes(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.nodes
    }

    pub fn edges(&self) -> &BTreeMap<String, EdgeKind> {
        &self.edges
    }
}

/// Reads the schemas a project defines for its maps.
pub trait SchemaStore {
    /// The schema of the given map kind in the project at `project`.
    fn schema(
        &self,
        project: &Path,
        kind: &str,
    ) -> impl Future<Output = Result<Schema>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(kinds: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
        kinds.iter().map(|k| (k.to_string(), BTreeSet::new())).collect()
    }

    fn edge(from: &str, to: &str) -> BTreeMap<String, EdgeKind> {
        BTreeMap::from([("links".to_string(), EdgeKind { from: from.into(), to: to.into() })])
    }

    #[test]
    fn a_schema_needs_a_node_kind() {
        let result = Schema::new("p".into(), "i".into(), nodes(&[]), BTreeMap::new());

        assert!(result.is_err());
    }

    #[test]
    fn an_edge_must_link_declared_node_kinds() {
        let declared = Schema::new("p".into(), "i".into(), nodes(&["a", "b"]), edge("a", "b"));
        let undeclared_to = Schema::new("p".into(), "i".into(), nodes(&["a"]), edge("a", "b"));
        let undeclared_from = Schema::new("p".into(), "i".into(), nodes(&["b"]), edge("a", "b"));

        assert!(declared.is_ok());
        assert!(undeclared_to.is_err());
        assert!(undeclared_from.is_err());
    }
}
