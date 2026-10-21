use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::core::{Definition, DefinitionStore, EdgeKind};
use crate::shared::Result;

/// Reads a definition from a TOML file.
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

impl DefinitionStore for TomlStore {
    async fn definition(&self, file: &Path) -> Result<Definition> {
        let name = file.display();

        let text = tokio::fs::read_to_string(file).await.map_err(|e| format!("{name}: {e}"))?;
        let parsed: File = toml::from_str(&text).map_err(|e| format!("{name}: {e}"))?;

        let mut nodes = BTreeMap::new();
        for (node, properties) in parsed.nodes {
            if let Some((property, _)) = properties.iter().find(|(_, value)| !value.is_empty()) {
                return Err(format!("{name}: property {property:?} of node kind {node:?} must be \"\"").into());
            }
            nodes.insert(node, properties.into_keys().collect::<BTreeSet<_>>());
        }
        let edges = parsed.edges.into_iter().map(|(k, e)| (k, EdgeKind { from: e.from, to: e.to })).collect();

        Definition::new(parsed.purpose, parsed.instructions, nodes, edges).map_err(|e| format!("{name}: {e}").into())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A definition file `notes.toml` holding the given text.
    fn file(text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("percept-definition-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.toml");
        std::fs::write(&file, text).unwrap();

        file
    }

    #[tokio::test]
    async fn reads_a_valid_file() {
        let file = file(
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

        let definition = TomlStore.definition(&file).await.unwrap();

        assert_eq!(definition.purpose(), "p");
        assert_eq!(definition.instructions(), "i");
        let properties = BTreeMap::from([("body".to_string(), "x".to_string()), ("by".to_string(), "y".to_string())]);
        assert!(definition.check_node("note", &properties).is_ok());
        assert!(definition.check_node("topic", &properties).is_err());
        assert!(definition.check_edge("about", "note", "topic").is_ok());
        assert!(definition.check_edge("about", "topic", "note").is_err());
    }

    #[tokio::test]
    async fn edges_may_be_absent() {
        let file = file("purpose = \"p\"\ninstructions = \"i\"\n[nodes.note]\n");

        let definition = TomlStore.definition(&file).await.unwrap();

        assert!(definition.check_edge("about", "note", "note").is_err());
    }

    #[tokio::test]
    async fn a_missing_file_is_an_error_naming_it() {
        let missing = file("").with_file_name("tasks.toml");

        let error = TomlStore.definition(&missing).await.err().unwrap();

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
            let file = file(text);

            let error = TomlStore.definition(&file).await.err().unwrap();

            assert!(error.to_string().contains("notes.toml"), "{text}: {error}");
        }
    }
}
