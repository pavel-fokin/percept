use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::shared::Result;

/// The shape of one kind of map: which nodes and edges it allows.
#[allow(dead_code, reason = "no caller reads this yet: purpose and instructions are for the agent, edges for edge validation")]
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

#[allow(dead_code, reason = "no caller reads this yet: purpose and instructions are for the agent, edges for edge validation")]
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

    /// A node must be of a declared kind and hold only properties that kind declares.
    pub fn check_node(&self, kind: &str, properties: &BTreeMap<String, String>) -> Result<()> {
        let Some(declared) = self.nodes.get(kind) else {
            let kinds: Vec<&str> = self.nodes.keys().map(String::as_str).collect();
            return Err(format!("unknown node kind {kind:?}; the schema declares {}", kinds.join(", ")).into());
        };

        if let Some(property) = properties.keys().find(|p| !declared.contains(*p)) {
            let allowed: Vec<&str> = declared.iter().map(String::as_str).collect();
            return Err(format!("node kind {kind:?} has no property {property:?}; it declares [{}]", allowed.join(", ")).into());
        }

        Ok(())
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
    fn a_node_must_be_a_declared_kind_with_declared_properties() {
        let nodes = BTreeMap::from([("note".to_string(), BTreeSet::from(["body".to_string()]))]);
        let schema = Schema::new("p".into(), "i".into(), nodes, BTreeMap::new()).unwrap();
        let body = BTreeMap::from([("body".to_string(), "x".to_string())]);
        let other = BTreeMap::from([("other".to_string(), "x".to_string())]);

        let subset = schema.check_node("note", &BTreeMap::new());
        let declared = schema.check_node("note", &body);
        let unknown_kind = schema.check_node("memo", &BTreeMap::new());
        let undeclared = schema.check_node("note", &other);

        assert!(subset.is_ok());
        assert!(declared.is_ok());
        assert!(unknown_kind.unwrap_err().to_string().contains("note"));
        assert!(undeclared.unwrap_err().to_string().contains("body"));
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
