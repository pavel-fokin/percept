//! Loads a project's cognitive-map schemas from TOML: `decisions` and
//! `tasks`, embedded in the binary, and
//! `<project>/.percept/schemas/*.toml`, which may replace a built-in
//! by name or declare a new map. `core` stays serde-free, so the
//! parsing and the checks a declared schema must pass live here.
//! `code` is never declared this way - it is derived from the working
//! tree, and a `code.toml` is refused; `index` is reserved for
//! `.percept/index.md`, the map directory, and an `index.toml` is
//! refused the same way. A project file that replaces a built-in may
//! only extend it: it must keep every node and edge kind, the same
//! `headlines`, and the same `settles` the built-in declares, since the
//! renderer and the write rules assume those kinds exist; it may add
//! more.

use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use crate::core::{Kind, Schema, Schemas, Settlement, CODE};

const DECISIONS_TOML: &str = include_str!("schemas/decisions.toml");
const TASKS_TOML: &str = include_str!("schemas/tasks.toml");

/// Where a project's own schema files live, under the project root
/// `checkout_root` finds.
const SCHEMAS_DIR: &str = ".percept/schemas";

/// `.percept/index.md` is the hand-written map directory; a schema by
/// this name would clobber it when rendered.
const INDEX: &str = "index";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
struct SettlesFile {
    by: String,
    of: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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
        let stem = file_name.strip_suffix(".toml").unwrap_or(&file_name);
        if stem == CODE {
            return Err(format!(
                "{file_name}: code is derived from the working tree, not declared"
            )
            .into());
        }
        if stem == INDEX {
            return Err(format!(
                "{file_name}: index.md is the hand-written map directory, not a declared map"
            )
            .into());
        }
        let schema = parse(&file_name, &text)?;
        match folded.iter().position(|built_in| built_in.name == schema.name) {
            Some(at) => {
                check_extends(&folded[at], &schema, &file_name)?;
                folded[at] = schema;
            }
            None => folded.push(schema),
        }
    }
    let mut schemas: Vec<Arc<Schema>> = folded.into_iter().map(Arc::new).collect();
    schemas.push(Arc::new(crate::core::code()));
    Ok(Schemas::new(schemas))
}

/// Every `*.toml` file directly under `<project>/.percept/schemas`,
/// name and contents, in a stable order. Empty when the directory does
/// not exist: a project need not declare any schema of its own. Only
/// regular files are read: a directory named `x.toml`, or a dangling
/// symlink an editor's lock file leaves behind, is skipped rather than
/// breaking every command that loads schemas.
fn project_files(project: &Path) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let dir = project.join(SCHEMAS_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|kind| kind.is_file()).unwrap_or(false))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".toml"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let text = std::fs::read_to_string(dir.join(&name))
                .map_err(|err| format!("{name}: {err}"))?;
            Ok((name, text))
        })
        .collect()
}

/// Refuses `project`, named by `file_name`, when it drops or changes
/// what `built_in` declares: a missing node or edge kind, or a
/// different `headlines` or `settles`. `project` may add more of
/// either; the names, not the glosses or `requires`, are what must
/// still match.
fn check_extends(
    built_in: &Schema,
    project: &Schema,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for (label, built_in_kinds, project_kinds) in [
        ("node kind", &built_in.node_kinds, &project.node_kinds),
        ("edge kind", &built_in.edge_kinds, &project.edge_kinds),
    ] {
        for kind in built_in_kinds {
            if !project_kinds.iter().any(|k| k.name == kind.name) {
                return Err(format!(
                    "{file_name}: drops {label} {:?}, which the built-in {:?} declares",
                    kind.name, built_in.name
                )
                .into());
            }
        }
    }
    if built_in.headline_kinds != project.headline_kinds {
        return Err(format!(
            "{file_name}: changes headlines from {:?} to {:?}",
            built_in.headline_kinds, project.headline_kinds
        )
        .into());
    }
    if built_in.settlement != project.settlement {
        return Err(format!(
            "{file_name}: changes settles from {:?} to {:?}",
            built_in.settlement, project.settlement
        )
        .into());
    }
    Ok(())
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

    check_kinds(file_name, "nodes", &file.nodes)?;
    check_kinds(file_name, "edges", &file.edges)?;

    let node_kinds: Vec<Kind> = file.nodes.into_iter().map(into_kind).collect();
    let edge_kinds: Vec<Kind> = file.edges.into_iter().map(into_kind).collect();
    let has_node_kind = |name: &str| node_kinds.iter().any(|kind| kind.name == name);

    let mut seen_headlines: Vec<&str> = Vec::new();
    for headline in &file.headlines {
        if !has_node_kind(headline) {
            return Err(format!(
                "{file_name}: headlines names {headline:?}, which is not a declared node kind"
            )
            .into());
        }
        if seen_headlines.contains(&headline.as_str()) {
            return Err(format!("{file_name}: headlines names {headline:?} twice").into());
        }
        seen_headlines.push(headline);
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

/// Refuses a blank kind name, a name repeated within `kinds`, or a
/// blank gloss or `requires` entry - `group` names the TOML array
/// (`nodes` or `edges`) in the error.
fn check_kinds(file_name: &str, group: &str, kinds: &[KindFile]) -> Result<(), Box<dyn std::error::Error>> {
    let mut seen: Vec<&str> = Vec::new();
    for kind in kinds {
        if kind.name.trim().is_empty() {
            return Err(format!("{file_name}: a {group} kind's name must not be blank").into());
        }
        if seen.contains(&kind.name.as_str()) {
            return Err(format!("{file_name}: {group} declares {:?} twice", kind.name).into());
        }
        seen.push(&kind.name);
        if kind.gloss.trim().is_empty() {
            return Err(format!(
                "{file_name}: {group} kind {:?} has a blank gloss",
                kind.name
            )
            .into());
        }
        if kind.requires.iter().any(|property| property.trim().is_empty()) {
            return Err(format!(
                "{file_name}: {group} kind {:?} requires a blank property",
                kind.name
            )
            .into());
        }
    }
    Ok(())
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
