//! Loads a project's cognitive-map schemas from TOML: `decisions` and
//! `tasks`, embedded in the binary, and
//! `<project>/.percept/schemas/*.toml`, which may replace a built-in
//! by name or declare a new map. `core` stays serde-free, so the
//! parsing and the checks a declared schema must pass live here.
//! `index` is reserved for `.percept/index.md`, the map directory, and
//! an `index.toml` is refused. A project file that replaces a built-in
//! may only extend it: it must keep every node and edge kind, the same
//! `headlines`, and the same `settles` the built-in declares, since the
//! renderer and the write rules assume those kinds exist; it may add
//! more.

use std::path::Path;

use serde::{Deserialize, Deserializer};

use crate::core::{default_prefix, EdgeKind, NodeKind, Schema, Schemas, Settlement};

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
    #[serde(default, rename = "node")]
    nodes: Vec<NodeFile>,
    #[serde(default, rename = "edge")]
    edges: Vec<EdgeFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettlesFile {
    by: String,
    of: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeFile {
    name: String,
    gloss: String,
    #[serde(default)]
    requires: Vec<String>,
    /// A node kind's short id prefix, `d` for `decision` - optional,
    /// since `default_prefix` covers the common case.
    prefix: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeFile {
    name: String,
    gloss: String,
    #[serde(deserialize_with = "one_or_many")]
    from: Vec<String>,
    #[serde(deserialize_with = "one_or_many")]
    to: Vec<String>,
}

/// An edge end as TOML may write it: one node kind's name, or a list of
/// several - `from = "decision"` or `to = ["function", "type"]`.
fn one_or_many<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(name) => vec![name],
        OneOrMany::Many(names) => names,
    })
}

/// Every schema `project` has: `decisions` and `tasks`, each replaced
/// by a project file of the same name when one exists, plus whatever
/// else `<project>/.percept/schemas` declares. Each error names the
/// file it came from.
pub fn load(project: &Path) -> Result<Schemas, Box<dyn std::error::Error>> {
    let mut folded = vec![parse("decisions", DECISIONS_TOML)?, parse("tasks", TASKS_TOML)?];
    for (stem, text) in project_files(project)? {
        if stem == INDEX {
            return Err(format!(
                "{stem}.toml: index.md is the hand-written map directory, not a declared map"
            )
            .into());
        }
        let schema = parse(&stem, &text)?;
        match folded.iter().position(|built_in| built_in.name == schema.name) {
            Some(at) => {
                check_extends(&folded[at], &schema, &stem)?;
                folded[at] = schema;
            }
            None => folded.push(schema),
        }
    }
    Ok(Schemas::new(folded))
}

/// Every `*.toml` file directly under `<project>/.percept/schemas`,
/// its stem and contents, in a stable order. Empty when the directory
/// does not exist: a project need not declare any schema of its own.
/// Only regular files are read: a directory named `x.toml`, or a
/// dangling symlink an editor's lock file leaves behind, is skipped
/// rather than breaking every command that loads schemas.
fn project_files(project: &Path) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let dir = project.join(SCHEMAS_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter_map(|name| name.strip_suffix(".toml").map(str::to_string))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|stem| {
            let text = std::fs::read_to_string(dir.join(format!("{stem}.toml")))
                .map_err(|err| format!("{stem}.toml: {err}"))?;
            Ok((stem, text))
        })
        .collect()
}

/// Refuses `project`, named by `stem`, when it drops or changes what
/// `built_in` declares: a missing node or edge kind, or a different
/// `headlines` or `settles`. `project` may add more of either; the
/// names, not the glosses or `requires`, are what must still match.
fn check_extends(
    built_in: &Schema,
    project: &Schema,
    stem: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for kind in &built_in.node_kinds {
        if project.node_kind(&kind.name).is_none() {
            return Err(format!(
                "{stem}.toml: drops node kind {:?}, which the built-in {:?} declares",
                kind.name, built_in.name
            )
            .into());
        }
    }
    for kind in &built_in.edge_kinds {
        if project.edge_kind(&kind.name).is_none() {
            return Err(format!(
                "{stem}.toml: drops edge kind {:?}, which the built-in {:?} declares",
                kind.name, built_in.name
            )
            .into());
        }
    }
    if built_in.headline_kinds != project.headline_kinds {
        return Err(format!(
            "{stem}.toml: changes headlines from {:?} to {:?}",
            built_in.headline_kinds, project.headline_kinds
        )
        .into());
    }
    if built_in.settlement != project.settlement {
        return Err(format!(
            "{stem}.toml: changes settles from {:?} to {:?}",
            built_in.settlement, project.settlement
        )
        .into());
    }
    Ok(())
}

/// Parses and validates one schema file: `stem` names it in every
/// error, so a project with several files knows which one is wrong.
fn parse(stem: &str, text: &str) -> Result<Schema, Box<dyn std::error::Error>> {
    let file: SchemaFile = toml::from_str(text).map_err(|err| format!("{stem}.toml: {err}"))?;

    if file.name != stem {
        return Err(format!(
            "{stem}.toml: declares name {:?}, which does not match the file name",
            file.name
        )
        .into());
    }
    if file.nodes.is_empty() {
        return Err(format!("{stem}.toml: declares no node kinds").into());
    }

    check_names_and_glosses(
        stem,
        "node",
        file.nodes.iter().map(|n| (n.name.as_str(), n.gloss.as_str())),
    )?;
    check_names_and_glosses(
        stem,
        "edge",
        file.edges.iter().map(|e| (e.name.as_str(), e.gloss.as_str())),
    )?;
    for node in &file.nodes {
        if node.requires.iter().any(|property| property.trim().is_empty()) {
            return Err(format!(
                "{stem}.toml: node kind {:?} requires a blank property",
                node.name
            )
            .into());
        }
    }

    let node_kinds: Vec<NodeKind> = file
        .nodes
        .into_iter()
        .map(|node| {
            let prefix = node.prefix.unwrap_or_else(|| default_prefix(&node.name));
            NodeKind {
                prefix,
                name: node.name,
                gloss: node.gloss,
                requires: node.requires,
            }
        })
        .collect();
    check_prefixes(stem, &node_kinds)?;

    let edge_kinds: Vec<EdgeKind> = file
        .edges
        .into_iter()
        .map(|edge| {
            check_edge_end(stem, &edge.name, "from", &edge.from, &node_kinds)?;
            check_edge_end(stem, &edge.name, "to", &edge.to, &node_kinds)?;
            Ok(EdgeKind {
                name: edge.name,
                gloss: edge.gloss,
                from: edge.from,
                to: edge.to,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    let mut schema = Schema {
        name: file.name,
        purpose: file.purpose,
        node_kinds,
        edge_kinds,
        headline_kinds: file.headlines.clone(),
        settlement: None,
    };

    for headline in &file.headlines {
        if schema.node_kind(headline).is_none() {
            return Err(format!(
                "{stem}.toml: headlines names {headline:?}, which is not a declared node kind"
            )
            .into());
        }
    }
    if let Some(headline) = repeated(file.headlines.iter().map(String::as_str)) {
        return Err(format!("{stem}.toml: headlines names {headline:?} twice").into());
    }

    if let Some(SettlesFile { by, of }) = file.settles {
        for (field, name) in [("by", &by), ("of", &of)] {
            if schema.node_kind(name).is_none() {
                return Err(format!(
                    "{stem}.toml: settles.{field} names {name:?}, which is not a declared node \
                     kind"
                )
                .into());
            }
        }
        schema.settlement = Some(Settlement { by, of });
    }

    Ok(schema)
}

/// Refuses a blank kind name, a name repeated within `kinds`, or a
/// blank gloss - `group` names the kind (`node` or `edge`) in the
/// error.
fn check_names_and_glosses<'a>(
    stem: &str,
    group: &str,
    kinds: impl Iterator<Item = (&'a str, &'a str)> + Clone,
) -> Result<(), Box<dyn std::error::Error>> {
    for (name, gloss) in kinds.clone() {
        if name.trim().is_empty() {
            return Err(format!("{stem}.toml: a {group} kind's name must not be blank").into());
        }
        if gloss.trim().is_empty() {
            return Err(format!("{stem}.toml: {group} kind {name:?} has a blank gloss").into());
        }
    }
    if let Some(name) = repeated(kinds.map(|(name, _)| name)) {
        return Err(format!("{stem}.toml: {group} declares {name:?} twice").into());
    }
    Ok(())
}

/// Refuses `end` (`from` or `to`) of edge kind `name` when it is empty,
/// names a blank kind, or names a node kind `node_kinds` does not
/// declare.
fn check_edge_end(
    stem: &str,
    edge: &str,
    end: &str,
    kinds: &[String],
    node_kinds: &[NodeKind],
) -> Result<(), Box<dyn std::error::Error>> {
    if kinds.is_empty() {
        return Err(format!("{stem}.toml: edge kind {edge:?}'s {end} names no node kind").into());
    }
    for kind in kinds {
        if kind.trim().is_empty() {
            return Err(format!(
                "{stem}.toml: edge kind {edge:?}'s {end} names a blank node kind"
            )
            .into());
        }
        if !node_kinds.iter().any(|node| node.name == *kind) {
            return Err(format!(
                "{stem}.toml: edge kind {edge:?}'s {end} names {kind:?}, which is not a \
                 declared node kind"
            )
            .into());
        }
    }
    Ok(())
}

/// Refuses two node kinds - explicit or defaulted - that resolve to
/// the same short id prefix: a schema load error, so the collision is
/// caught once, not the first time two nodes' short ids clash.
fn check_prefixes(stem: &str, node_kinds: &[NodeKind]) -> Result<(), Box<dyn std::error::Error>> {
    let mut seen: Vec<&NodeKind> = Vec::new();
    for kind in node_kinds {
        if let Some(other) = seen.iter().find(|other| other.prefix == kind.prefix) {
            return Err(format!(
                "{stem}.toml: node kinds {:?} and {:?} both take the short id prefix {:?}",
                other.name, kind.name, kind.prefix
            )
            .into());
        }
        seen.push(kind);
    }
    Ok(())
}

/// The first name `names` repeats, if any - the one duplicate scan
/// every kind and headline check shares.
fn repeated<'a>(names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen: Vec<&str> = Vec::new();
    for name in names {
        if seen.contains(&name) {
            return Some(name);
        }
        seen.push(name);
    }
    None
}

#[cfg(test)]
mod tests;
