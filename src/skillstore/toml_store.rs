use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::core::{EdgeKind, Schema, Skill, SkillStore};
use crate::shared::Result;

/// Reads a skill from a TOML file.
pub struct TomlStore;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    purpose: String,
    instructions: String,
    nodes: Option<BTreeMap<String, BTreeMap<String, String>>>,
    edges: Option<BTreeMap<String, Edge>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    from: String,
    to: String,
}

/// A schema from the declared node and edge kinds; every property value must be `""`.
fn schema(declared: BTreeMap<String, BTreeMap<String, String>>, edges: BTreeMap<String, Edge>) -> Result<Schema> {
    let mut nodes = BTreeMap::new();
    for (node, properties) in declared {
        if let Some((property, _)) = properties.iter().find(|(_, value)| !value.is_empty()) {
            return Err(format!("property {property:?} of node kind {node:?} must be \"\"").into());
        }
        nodes.insert(node, properties.into_keys().collect::<BTreeSet<_>>());
    }
    let edges = edges.into_iter().map(|(k, e)| (k, EdgeKind { from: e.from, to: e.to })).collect();

    Schema::new(nodes, edges)
}

impl SkillStore for TomlStore {
    async fn skill(&self, path: &Path) -> Result<Skill> {
        let name = path.display();

        let text = tokio::fs::read_to_string(path).await.map_err(|e| format!("{name}: {e}"))?;
        let parsed: File = toml::from_str(&text).map_err(|e| format!("{name}: {e}"))?;

        let schema = match (parsed.nodes, parsed.edges) {
            (None, None) => None,
            (nodes, edges) => Some(schema(nodes.unwrap_or_default(), edges.unwrap_or_default()).map_err(|e| format!("{name}: {e}"))?),
        };

        Ok(Skill::new(parsed.purpose, parsed.instructions, schema))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A skill file `notes.toml` holding the given text.
    fn file(text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("percept-skill-{}", uuid::Uuid::now_v7()));
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

        let skill = TomlStore.skill(&file).await.unwrap();

        assert_eq!(skill.description(), "p");
        assert_eq!(skill.instructions(), "i");
        let properties = BTreeMap::from([("body".to_string(), "x".to_string()), ("by".to_string(), "y".to_string())]);
        assert!(skill.check_node("note", &properties).is_ok());
        assert!(skill.check_node("topic", &properties).is_err());
        assert!(skill.check_edge("about", "note", "topic").is_ok());
        assert!(skill.check_edge("about", "topic", "note").is_err());
    }

    #[tokio::test]
    async fn edges_may_be_absent() {
        let file = file("purpose = \"p\"\ninstructions = \"i\"\n[nodes.note]\n");

        let skill = TomlStore.skill(&file).await.unwrap();

        assert!(skill.check_edge("about", "note", "note").is_err());
    }

    #[tokio::test]
    async fn a_file_without_nodes_or_edges_has_no_schema() {
        let file = file("purpose = \"p\"\ninstructions = \"i\"\n");

        let skill = TomlStore.skill(&file).await.unwrap();

        assert!(skill.schema().is_none());
        assert!(skill.check_node("any", &BTreeMap::from([("p".to_string(), "x".to_string())])).is_ok());
    }

    #[tokio::test]
    async fn a_missing_file_is_an_error_naming_it() {
        let missing = file("").with_file_name("tasks.toml");

        let error = TomlStore.skill(&missing).await.err().unwrap();

        assert!(error.to_string().contains("tasks.toml"));
    }

    #[tokio::test]
    async fn a_bad_file_is_an_error_naming_it() {
        let cases = [
            "purpose = \"p\"\ninstructions = \"i\"\nextra = 1\n[nodes.note]\n",
            "purpose = \"p\"\ninstructions = \"i\"\n[nodes.note]\nbody = \"text\"\n",
            "purpose = \"p\"\ninstructions = \"i\"\n[nodes.note]\n[edges.about]\nfrom = \"note\"\nto = \"topic\"\n",
            "purpose = \"p\"\ninstructions = \"i\"\n[edges.about]\nfrom = \"note\"\nto = \"note\"\n",
            "purpose = \"p\"\ninstructions = \"i\"\n[nodes]\n",
            "purpose = \"p\"\n[nodes.note]\n",
        ];

        for text in cases {
            let file = file(text);

            let error = TomlStore.skill(&file).await.err().unwrap();

            assert!(error.to_string().contains("notes.toml"), "{text}: {error}");
        }
    }
}
