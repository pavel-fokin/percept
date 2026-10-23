use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::Path;

use serde::Deserialize;

use crate::core::{EdgeKind, Schema, Skill, SkillStore};
use crate::shared::Result;

/// The `metadata` key of SKILL.md that names the schema file of the map the skill produces.
const SCHEMA_KEY: &str = "percept-schema";

/// Reads a skill from a directory in the agentskills.io format.
#[derive(Clone)]
pub struct MarkdownStore;

#[derive(Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
    #[serde(default)]
    metadata: BTreeMap<String, serde_yaml_ng::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaFile {
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

/// The YAML between the leading `---` line and the next one, and the text after it.
fn split(text: &str) -> Result<(&str, &str)> {
    let Some(("---", rest)) = text.split_once('\n').map(|(first, rest)| (first.trim_end(), rest)) else {
        return Err("must start with a --- line".into());
    };

    let mut end = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Ok((&rest[..end], &rest[end + line.len()..]));
        }
        end += line.len();
    }

    Err("frontmatter is not closed by a --- line".into())
}

impl SkillStore for MarkdownStore {
    async fn skill(&self, path: &Path) -> Result<Skill> {
        let file = path.join("SKILL.md");
        let text = tokio::fs::read_to_string(&file).await.map_err(|e| format!("{}: {e}", file.display()))?;
        let (yaml, body) = split(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", file.display()))?;
        let front: Frontmatter = serde_yaml_ng::from_str(yaml).map_err(|e| format!("{}: {e}", file.display()))?;
        if front.name.is_empty() || front.description.is_empty() {
            return Err(format!("{}: name and description must not be empty", file.display()).into());
        }
        if path.file_name().and_then(|n| n.to_str()) != Some(front.name.as_str()) {
            return Err(format!("{}: name {:?} must match the directory name", file.display(), front.name).into());
        }

        let schema = match front.metadata.get(SCHEMA_KEY) {
            None => None,
            Some(declared) => {
                let Some(relative) = declared.as_str().map(Path::new).filter(|p| p.is_relative()) else {
                    return Err(format!("{}: {SCHEMA_KEY} must be a path relative to the skill directory", file.display()).into());
                };
                let file = path.join(relative);
                let text = tokio::fs::read_to_string(&file).await.map_err(|e| format!("{}: {e}", file.display()))?;
                let parsed: SchemaFile = toml::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;

                Some(schema(parsed.nodes, parsed.edges).map_err(|e| format!("{}: {e}", file.display()))?)
            }
        };

        Ok(Skill::new(front.name, front.description, body.trim().to_string(), schema))
    }

    async fn skills(&self, library: &Path) -> Result<Vec<(String, Result<Skill>)>> {
        let mut entries = match tokio::fs::read_dir(library).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("{}: {e}", library.display()).into()),
        };

        let mut skills = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') && tokio::fs::metadata(entry.path()).await.is_ok_and(|m| m.is_dir()) {
                let skill = self.skill(&entry.path()).await;
                skills.push((name, skill));
            }
        }
        skills.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(skills)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A skill directory `notes` holding the given SKILL.md and, when given, assets/schema.toml.
    fn dir(skill: Option<&str>, schema: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("percept-skill-{}", uuid::Uuid::now_v7())).join("notes");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        if let Some(skill) = skill {
            std::fs::write(dir.join("SKILL.md"), skill).unwrap();
        }
        if let Some(schema) = schema {
            std::fs::write(dir.join("assets").join("schema.toml"), schema).unwrap();
        }

        dir
    }

    const SKILL: &str = "---\nname: notes\ndescription: p\n---\n\ni\n\n";

    const WITH_SCHEMA: &str = "---\nname: notes\ndescription: p\nmetadata:\n  percept-schema: assets/schema.toml\n---\n\ni\n\n";

    #[tokio::test]
    async fn reads_a_valid_skill() {
        let dir = dir(
            Some(WITH_SCHEMA),
            Some(
                r#"
                [nodes.note]
                body = ""
                by = ""

                [nodes.topic]

                [edges.about]
                from = "note"
                to = "topic"
                "#,
            ),
        );

        let skill = MarkdownStore.skill(&dir).await.unwrap();

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
        let dir = dir(Some(WITH_SCHEMA), Some("[nodes.note]\n"));

        let skill = MarkdownStore.skill(&dir).await.unwrap();

        assert!(skill.check_edge("about", "note", "note").is_err());
    }

    #[tokio::test]
    async fn a_skill_that_declares_no_schema_has_none() {
        let dir = dir(Some(SKILL), Some("[nodes.note]\n"));

        let skill = MarkdownStore.skill(&dir).await.unwrap();

        assert!(skill.schema().is_none());
        assert!(skill.check_node("any", &BTreeMap::from([("p".to_string(), "x".to_string())])).is_ok());
    }

    #[tokio::test]
    async fn other_spec_keys_are_accepted() {
        let dir = dir(
            Some("---\nname: notes\ndescription: p\nlicense: MIT\ncompatibility: any\nmetadata:\n  author: me\nallowed-tools: Bash\n---\ni"),
            None,
        );

        let skill = MarkdownStore.skill(&dir).await.unwrap();

        assert_eq!(skill.instructions(), "i");
    }

    #[tokio::test]
    async fn a_missing_skill_file_is_an_error_naming_it() {
        let dir = dir(None, None);

        let error = MarkdownStore.skill(&dir).await.err().unwrap();

        assert!(error.to_string().contains("SKILL.md"));
    }

    #[tokio::test]
    async fn a_bad_skill_file_is_an_error_naming_it() {
        let cases = [
            "no frontmatter",
            "---\nname: notes\ndescription: p\nbody",
            "---\ndescription: p\n---\ni",
            "---\nname: notes\n---\ni",
            "---\nname: \"\"\ndescription: p\n---\ni",
            "---\nname: notes\ndescription: \"\"\n---\ni",
            "---\nname: other\ndescription: p\n---\ni",
            "---\nname: [\ndescription: p\n---\ni",
        ];

        for text in cases {
            let dir = dir(Some(text), None);

            let error = MarkdownStore.skill(&dir).await.err().unwrap();

            assert!(error.to_string().contains("SKILL.md"), "{text}: {error}");
        }
    }

    #[tokio::test]
    async fn a_library_lists_its_skill_directories_by_name() {
        let library = std::env::temp_dir().join(format!("percept-library-{}", uuid::Uuid::now_v7()));
        for name in ["zeta", "alpha"] {
            std::fs::create_dir_all(library.join(name)).unwrap();
            std::fs::write(library.join(name).join("SKILL.md"), format!("---\nname: {name}\ndescription: d\n---\ni")).unwrap();
        }
        std::fs::create_dir_all(library.join("broken")).unwrap();
        std::fs::write(library.join(".DS_Store"), "").unwrap();

        let skills = MarkdownStore.skills(&library).await.unwrap();

        let names: Vec<_> = skills.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["alpha", "broken", "zeta"]);
        assert_eq!(skills[0].1.as_ref().unwrap().name(), "alpha");
        assert!(skills[1].1.as_ref().err().unwrap().to_string().contains("SKILL.md"));
        assert!(skills[2].1.is_ok());
    }

    #[tokio::test]
    async fn a_missing_library_holds_no_skills() {
        let library = std::env::temp_dir().join(format!("percept-library-{}", uuid::Uuid::now_v7()));

        let skills = MarkdownStore.skills(&library).await.unwrap();

        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn a_bad_schema_file_is_an_error_naming_it() {
        let cases = [
            "extra = 1\n[nodes.note]\n",
            "[nodes.note]\nbody = \"text\"\n",
            "[nodes.note]\n[edges.about]\nfrom = \"note\"\nto = \"topic\"\n",
            "[edges.about]\nfrom = \"note\"\nto = \"note\"\n",
            "[nodes]\n",
            "",
        ];

        for text in cases {
            let dir = dir(Some(WITH_SCHEMA), Some(text));

            let error = MarkdownStore.skill(&dir).await.err().unwrap();

            assert!(error.to_string().contains("schema.toml"), "{text}: {error}");
        }
    }

    #[tokio::test]
    async fn a_declared_schema_file_that_is_missing_is_an_error_naming_it() {
        let dir = dir(Some(WITH_SCHEMA), None);

        let error = MarkdownStore.skill(&dir).await.err().unwrap();

        assert!(error.to_string().contains("schema.toml"));
    }

    #[tokio::test]
    async fn a_schema_path_that_is_not_relative_is_an_error() {
        let cases = ["/tmp/schema.toml", "[assets/schema.toml]"];

        for path in cases {
            let dir = dir(Some(&format!("---\nname: notes\ndescription: p\nmetadata:\n  percept-schema: {path}\n---\ni")), None);

            let error = MarkdownStore.skill(&dir).await.err().unwrap();

            assert!(error.to_string().contains("percept-schema"), "{path}: {error}");
        }
    }
}
