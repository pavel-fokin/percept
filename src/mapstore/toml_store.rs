use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::core::{EdgeKind, Schema, SchemaStore};
use crate::shared::Result;

/// Reads `<project>/.percept/schemas/<kind>.toml`.
pub struct TomlStore;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    purpose: String,
    instructions: String,
    #[serde(default)]
    nodes: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    edges: BTreeMap<String, Edge>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    from: String,
    to: String,
}

impl SchemaStore for TomlStore {
    async fn schema(&self, project: &Path, kind: &str) -> Result<Schema> {
        let path = project.join(".percept").join("schemas").join(format!("{kind}.toml"));
        let name = path.display();

        let text = tokio::fs::read_to_string(&path).await.map_err(|e| format!("{name}: {e}"))?;
        let file: File = toml::from_str(&text).map_err(|e| format!("{name}: {e}"))?;

        let mut nodes = BTreeMap::new();
        for (node, properties) in file.nodes {
            if let Some((property, _)) = properties.iter().find(|(_, value)| !value.is_empty()) {
                return Err(format!("{name}: property {property:?} of node kind {node:?} must be \"\"").into());
            }
            nodes.insert(node, properties.into_keys().collect::<BTreeSet<_>>());
        }
        let edges = file.edges.into_iter().map(|(k, e)| (k, EdgeKind { from: e.from, to: e.to })).collect();

        Schema::new(file.purpose, file.instructions, nodes, edges).map_err(|e| format!("{name}: {e}").into())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A project directory holding one schema file with the given text.
    fn project(text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("percept-schema-{}", uuid::Uuid::now_v7()));
        let schemas = dir.join(".percept").join("schemas");
        std::fs::create_dir_all(&schemas).unwrap();
        std::fs::write(schemas.join("notes.toml"), text).unwrap();

        dir
    }

    #[tokio::test]
    async fn reads_a_valid_file() {
        let dir = project(
            r#"
            purpose = "p"
            instructions = "i"

            [nodes.note]
            body = ""
            by = ""

            [nodes.topic]

            [edges.about]
            from = "note"
            to = "topic"
            "#,
        );

        let schema = TomlStore.schema(&dir, "notes").await.unwrap();

        assert_eq!(schema.purpose(), "p");
        assert_eq!(schema.instructions(), "i");
        let properties = BTreeMap::from([("body".to_string(), "x".to_string()), ("by".to_string(), "y".to_string())]);
        assert!(schema.check_node("note", &properties).is_ok());
        assert!(schema.check_node("topic", &properties).is_err());
        assert!(schema.check_edge("about", "note", "topic").is_ok());
        assert!(schema.check_edge("about", "topic", "note").is_err());
    }

    #[tokio::test]
    async fn edges_may_be_absent() {
        let dir = project("purpose = \"p\"\ninstructions = \"i\"\n[nodes.note]\n");

        let schema = TomlStore.schema(&dir, "notes").await.unwrap();

        assert!(schema.check_edge("about", "note", "note").is_err());
    }

    #[tokio::test]
    async fn a_missing_file_is_an_error_naming_it() {
        let dir = project("");

        let error = TomlStore.schema(&dir, "tasks").await.err().unwrap();

        assert!(error.to_string().contains("tasks.toml"));
    }

    #[tokio::test]
    async fn a_bad_file_is_an_error_naming_it() {
        let cases = [
            "purpose = \"p\"\ninstructions = \"i\"\nextra = 1\n[nodes.note]\n",
            "purpose = \"p\"\ninstructions = \"i\"\n[nodes.note]\nbody = \"text\"\n",
            "purpose = \"p\"\ninstructions = \"i\"\n[nodes.note]\n[edges.about]\nfrom = \"note\"\nto = \"topic\"\n",
            "purpose = \"p\"\ninstructions = \"i\"\n",
            "purpose = \"p\"\n[nodes.note]\n",
        ];

        for text in cases {
            let dir = project(text);

            let error = TomlStore.schema(&dir, "notes").await.err().unwrap();

            assert!(error.to_string().contains("notes.toml"), "{text}: {error}");
        }
    }
}
