//! Loads a project's cognitive-map schemas from
//! `<project>/.percept/schemas/*.toml`. `core` stays serde-free, so
//! the parsing and the checks a declared schema must pass live here. A
//! project with no such directory, or none in it, declares no maps at
//! all: `load` returns an empty `Schemas`, and `percept init <client>`
//! is what gives a fresh checkout its first schema files, copied from
//! the `decisions` and `concepts` templates this binary embeds - see
//! `templates`.

use std::path::Path;

use serde::{Deserialize, Deserializer};

use crate::core::{default_prefix, EdgeKind, NodeKind, Schema, Schemas};

/// The two schema templates `percept init <client>` copies into a
/// fresh checkout's `SCHEMAS_DIR`, as `(<name>, <text>)`. Nothing else
/// reads these; a loaded project's schemas come only from `load`, over
/// the files `init` or the project's own author wrote.
pub const TEMPLATES: [(&str, &str); 2] = [
    ("decisions", include_str!("schemas/decisions.toml")),
    ("concepts", include_str!("schemas/concepts.toml")),
];

/// Where a project's schema files live, under the project root
/// `checkout_root` finds - what `load` reads and `init` writes.
pub const SCHEMAS_DIR: &str = ".percept/schemas";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaFile {
    name: String,
    purpose: String,
    #[serde(default)]
    headlines: Vec<String>,
    #[serde(default, rename = "node")]
    nodes: Vec<NodeFile>,
    #[serde(default, rename = "edge")]
    edges: Vec<EdgeFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeFile {
    kind: String,
    #[serde(default)]
    gloss: String,
    #[serde(default)]
    requires: Vec<String>,
    /// A node kind's short id prefix, `d` for `decision` - optional,
    /// since `default_prefix` covers the common case.
    prefix: Option<String>,
    /// The values a `state` property on a node of this kind may hold -
    /// `state = ["open", "done", "dropped"]`, a set with no value open
    /// by position. Empty when the kind carries no state.
    #[serde(default, rename = "state")]
    states: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeFile {
    kind: String,
    #[serde(default)]
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

/// Every schema `<project>/.percept/schemas` declares, in the stable
/// order `project_files` gives - empty when the directory is missing
/// or holds no `.toml` file, so a project that has not run `percept
/// init <client>` yet has no maps at all. Each error names the file it
/// came from.
pub fn load(project: &Path) -> Result<Schemas, Box<dyn std::error::Error>> {
    let folded = project_files(project)?
        .into_iter()
        .map(|(stem, text)| parse(&stem, &text))
        .collect::<Result<Vec<_>, _>>()?;
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

    check_kinds(stem, "node", file.nodes.iter().map(|n| n.kind.as_str()))?;
    check_kinds(stem, "edge", file.edges.iter().map(|e| e.kind.as_str()))?;
    for node in &file.nodes {
        if node.requires.iter().any(|property| property.trim().is_empty()) {
            return Err(format!(
                "{stem}.toml: node kind {:?} requires a blank property",
                node.kind
            )
            .into());
        }
        if !node.states.is_empty() {
            if node.states.len() < 2 {
                return Err(format!(
                    "{stem}.toml: node kind {:?} declares fewer than two states",
                    node.kind
                )
                .into());
            }
            if node.states.iter().any(|state| state.trim().is_empty()) {
                return Err(format!(
                    "{stem}.toml: node kind {:?} declares a blank state",
                    node.kind
                )
                .into());
            }
            if let Some(state) = repeated(node.states.iter().map(String::as_str)) {
                return Err(format!(
                    "{stem}.toml: node kind {:?} declares the state {state:?} twice",
                    node.kind
                )
                .into());
            }
        }
    }

    let node_kinds: Vec<NodeKind> = file
        .nodes
        .into_iter()
        .map(|node| {
            let prefix = node.prefix.unwrap_or_else(|| default_prefix(&node.kind));
            NodeKind {
                prefix,
                kind: node.kind,
                gloss: node.gloss.trim().to_string(),
                requires: node.requires,
                states: node.states,
            }
        })
        .collect();
    check_prefixes(stem, &node_kinds)?;

    let edge_kinds: Vec<EdgeKind> = file
        .edges
        .into_iter()
        .map(|edge| {
            check_edge_end(stem, &edge.kind, "from", &edge.from, &node_kinds)?;
            check_edge_end(stem, &edge.kind, "to", &edge.to, &node_kinds)?;
            Ok(EdgeKind {
                kind: edge.kind,
                gloss: edge.gloss.trim().to_string(),
                from: edge.from,
                to: edge.to,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    let schema = Schema {
        name: file.name,
        purpose: file.purpose,
        node_kinds,
        edge_kinds,
        headline_kinds: file.headlines.clone(),
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

    Ok(schema)
}

/// Refuses a blank kind name, or a name repeated within `kinds` -
/// `group` names the kind (`node` or `edge`) in the error.
fn check_kinds<'a>(
    stem: &str,
    group: &str,
    kinds: impl Iterator<Item = &'a str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut names: Vec<&str> = Vec::new();
    for name in kinds {
        if name.trim().is_empty() {
            return Err(format!("{stem}.toml: a {group} kind must not be blank").into());
        }
        names.push(name);
    }
    if let Some(name) = repeated(names.into_iter()) {
        return Err(format!("{stem}.toml: {group} declares {name:?} twice").into());
    }
    Ok(())
}

/// Refuses `end` (`from` or `to`) of edge kind `name` when it is empty
/// or names a node kind `node_kinds` does not declare - a blank name
/// falls in the latter, since no declared kind is blank.
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
        if !node_kinds.iter().any(|node| node.kind == *kind) {
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
                other.kind, kind.kind, kind.prefix
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
