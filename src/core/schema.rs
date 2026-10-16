use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::shared::Result;

/// The node kind of a citation. Every map has it, and no schema declares it.
const CITATION: &str = "citation";

/// The edge kind from a note to a citation. Every map has it, and no schema declares it.
pub const CITES: &str = "cites";

/// The shape of one kind of map: which nodes and edges it allows.
pub struct Schema {
    #[allow(dead_code, reason = "parsed now; read when the agent is shown the schema")]
    purpose: String,
    #[allow(dead_code, reason = "parsed now; read when the agent is shown the schema")]
    instructions: String,
    nodes: BTreeMap<String, BTreeSet<String>>,
    edges: BTreeMap<String, EdgeKind>,
}

/// The node kinds an edge kind links.
pub struct EdgeKind {
    pub from: String,
    pub to: String,
}

impl Schema {
    /// A schema needs a node kind, and each edge kind links declared node kinds.
    /// The built-in `citation` node kind and `cites` edge kind are not its to declare.
    pub fn new(
        purpose: String,
        instructions: String,
        nodes: BTreeMap<String, BTreeSet<String>>,
        edges: BTreeMap<String, EdgeKind>,
    ) -> Result<Self> {
        if nodes.is_empty() {
            return Err("a schema needs at least one node kind".into());
        }

        if nodes.contains_key(CITATION) {
            return Err(format!("the node kind {CITATION:?} is built in; a schema cannot declare it").into());
        }

        if edges.contains_key(CITES) {
            return Err(format!("the edge kind {CITES:?} is built in; a schema cannot declare it").into());
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

    /// An edge must be of a declared kind and link the node kinds that kind declares.
    /// A `cites` edge runs from a declared node kind to a citation.
    pub fn check_edge(&self, kind: &str, from: &str, to: &str) -> Result<()> {
        if kind == CITES {
            if !self.nodes.contains_key(from) || to != CITATION {
                return Err(format!("edge {CITES:?} runs from a note to a {CITATION}, not from {from:?} to {to:?}").into());
            }

            return Ok(());
        }

        let Some(declared) = self.edges.get(kind) else {
            let kinds: Vec<&str> = self.edges.keys().map(String::as_str).collect();
            return Err(format!("unknown edge kind {kind:?}; the schema declares [{}]", kinds.join(", ")).into());
        };

        for (end, expected, actual) in [("from", &declared.from, from), ("to", &declared.to, to)] {
            if expected != actual {
                return Err(format!("edge {kind:?} expects a {expected:?} node at {end}, not {actual:?}").into());
            }
        }

        Ok(())
    }

    #[allow(dead_code, reason = "parsed now; read when the agent is shown the schema")]
    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    #[allow(dead_code, reason = "parsed now; read when the agent is shown the schema")]
    pub fn instructions(&self) -> &str {
        &self.instructions
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
    fn an_edge_must_be_a_declared_kind_between_its_declared_node_kinds() {
        let schema = Schema::new("p".into(), "i".into(), nodes(&["a", "b"]), edge("a", "b")).unwrap();

        let valid = schema.check_edge("links", "a", "b");
        let unknown = schema.check_edge("other", "a", "b");
        let wrong_from = schema.check_edge("links", "b", "b");
        let wrong_to = schema.check_edge("links", "a", "a");

        assert!(valid.is_ok());
        assert!(unknown.unwrap_err().to_string().contains("links"));
        assert!(wrong_from.unwrap_err().to_string().contains("from"));
        assert!(wrong_to.unwrap_err().to_string().contains("to"));
    }

    #[test]
    fn a_schema_cannot_declare_the_built_in_citation_or_cites() {
        let citation = Schema::new("p".into(), "i".into(), nodes(&["a", "citation"]), BTreeMap::new());
        let cites = Schema::new("p".into(), "i".into(), nodes(&["a"]), BTreeMap::from([("cites".to_string(), EdgeKind { from: "a".into(), to: "a".into() })]));

        assert!(citation.err().unwrap().to_string().contains("citation"));
        assert!(cites.err().unwrap().to_string().contains("cites"));
    }

    #[test]
    fn a_cites_edge_runs_only_from_a_note_to_a_citation() {
        let schema = Schema::new("p".into(), "i".into(), nodes(&["a", "b"]), edge("a", "b")).unwrap();

        let valid = schema.check_edge("cites", "a", "citation");
        let to_note = schema.check_edge("cites", "a", "b");
        let from_citation = schema.check_edge("cites", "citation", "citation");
        let from_unknown = schema.check_edge("cites", "z", "citation");

        assert!(valid.is_ok());
        assert!(to_note.is_err());
        assert!(from_citation.is_err());
        assert!(from_unknown.is_err());
    }

    #[test]
    fn a_schema_edge_cannot_reach_a_citation() {
        let schema = Schema::new("p".into(), "i".into(), nodes(&["a", "b"]), edge("a", "b")).unwrap();

        let result = schema.check_edge("links", "a", "citation");

        assert!(result.is_err());
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
