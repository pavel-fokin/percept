//! Loads a project's cognitive-map schemas from TOML: `decisions` and
//! `tasks`, embedded in the binary, and
//! `<project>/.percept/schemas/*.toml`, which may replace a built-in
//! by name or declare a new map. `core` stays serde-free, so the
//! parsing and the checks a declared schema must pass live here.
//! `code` is never declared this way - it is derived from the working
//! tree, and a `code.toml` is refused.

use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use crate::core::{Kind, Schema, Schemas, Settlement};

const DECISIONS_TOML: &str = include_str!("schemas/decisions.toml");
const TASKS_TOML: &str = include_str!("schemas/tasks.toml");

/// Where a project's own schema files live, under the project root
/// `checkout_root` finds.
const SCHEMAS_DIR: &str = ".percept/schemas";

/// `code` is derived from the working tree; a file by this name would
/// claim to declare it.
const CODE_FILE: &str = "code.toml";

#[derive(Deserialize)]
struct SchemaFile {
    name: String,
    purpose: String,
    #[serde(default)]
    headlines: Vec<String>,
    settles: Option<SettlesFile>,
    #[serde(default)]
    nodes: Vec<KindFile>,
    #[serde(default)]
    edges: Vec<KindFile>,
}

#[derive(Deserialize)]
struct SettlesFile {
    by: String,
    of: String,
}

#[derive(Deserialize)]
struct KindFile {
    name: String,
    gloss: String,
    #[serde(default)]
    requires: Vec<String>,
}

/// Every schema `project` has: `decisions` and `tasks`, each replaced
/// by a project file of the same name when one exists, plus whatever
/// else `<project>/.percept/schemas` declares, and `code`, always
/// derived. Each error names the file it came from.
pub fn load(project: &Path) -> Result<Schemas, Box<dyn std::error::Error>> {
    let mut folded = vec![
        parse("decisions.toml", DECISIONS_TOML)?,
        parse("tasks.toml", TASKS_TOML)?,
    ];
    for (file_name, text) in project_files(project)? {
        if file_name == CODE_FILE {
            return Err(format!(
                "{file_name}: code is derived from the working tree, not declared"
            )
            .into());
        }
        let schema = parse(&file_name, &text)?;
        match folded.iter().position(|built_in| built_in.name == schema.name) {
            Some(at) => folded[at] = schema,
            None => folded.push(schema),
        }
    }
    let folded = folded.into_iter().map(Arc::new).collect();
    Ok(Schemas::new(folded, Arc::new(crate::core::code())))
}

/// Every `*.toml` file directly under `<project>/.percept/schemas`,
/// name and contents, in a stable order. Empty when the directory
/// does not exist: a project need not declare any schema of its own.
fn project_files(project: &Path) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let dir = project.join(SCHEMAS_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".toml"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let text = std::fs::read_to_string(dir.join(&name))?;
            Ok((name, text))
        })
        .collect()
}

/// Parses and validates one schema file: `file_name` names it in every
/// error, so a project with several files knows which one is wrong.
fn parse(file_name: &str, text: &str) -> Result<Schema, Box<dyn std::error::Error>> {
    let file: SchemaFile = toml::from_str(text).map_err(|err| format!("{file_name}: {err}"))?;

    let stem = file_name.strip_suffix(".toml").unwrap_or(file_name);
    if file.name != stem {
        return Err(format!(
            "{file_name}: declares name {:?}, which does not match the file name",
            file.name
        )
        .into());
    }
    if file.nodes.is_empty() {
        return Err(format!("{file_name}: declares no node kinds").into());
    }

    let node_kinds: Vec<Kind> = file.nodes.into_iter().map(into_kind).collect();
    let edge_kinds: Vec<Kind> = file.edges.into_iter().map(into_kind).collect();
    let has_node_kind = |name: &str| node_kinds.iter().any(|kind| kind.name == name);

    for headline in &file.headlines {
        if !has_node_kind(headline) {
            return Err(format!(
                "{file_name}: headlines names {headline:?}, which is not a declared node kind"
            )
            .into());
        }
    }
    let settlement = match file.settles {
        Some(SettlesFile { by, of }) => {
            for (field, name) in [("by", &by), ("of", &of)] {
                if !has_node_kind(name) {
                    return Err(format!(
                        "{file_name}: settles.{field} names {name:?}, which is not a declared \
                         node kind"
                    )
                    .into());
                }
            }
            Some(Settlement { by, of })
        }
        None => None,
    };

    Ok(Schema {
        name: file.name,
        purpose: file.purpose,
        node_kinds,
        edge_kinds,
        headline_kinds: file.headlines,
        settlement,
        derived: false,
    })
}

fn into_kind(file: KindFile) -> Kind {
    Kind {
        name: file.name,
        gloss: file.gloss,
        requires: file.requires,
    }
}

#[cfg(test)]
mod tests;
