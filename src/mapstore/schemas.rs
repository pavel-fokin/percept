//! Loads a project's cognitive-map schemas from
//! `<project>/.percept/schemas/*.toml`. `core` stays serde-free, so
//! the parsing and the checks a declared schema must pass live here. A
//! project with no such directory, or none in it, declares no maps at
//! all: `load` returns an empty `Schemas`, and `percept init <client>`
//! is what gives a fresh checkout its first schema file, copied from
//! the templates this binary embeds - see `templates`.

use std::path::Path;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};

use crate::core::{default_prefix, EdgeKind, NodeKind, Rules, Schema, Schemas};

/// A session's start - `percept start`'s own moment, and the hook's
/// `SessionStart`.
pub const SESSION_STARTED: &str = "session.started";
/// A prompt reaching the log - the hook's `UserPromptSubmit`.
pub const MESSAGE_RECEIVED: &str = "message.received";
/// A reflection's own opening moment.
pub const REFLECTION_STARTED: &str = "reflection.started";

/// The moments a schema may declare `[rules]` lines for - the only
/// three percept has a channel to speak on. `mapstore` is the layer
/// that knows this, not `core`: a schema's `Rules` holds whatever a
/// loader gives it, and `store` names its own event kinds without this
/// list depending on them.
pub const MOMENTS: [&str; 3] = [SESSION_STARTED, MESSAGE_RECEIVED, REFLECTION_STARTED];

include!(concat!(env!("OUT_DIR"), "/schema_templates.rs"));

/// The schema templates `percept init <client>` copies into a fresh
/// checkout's `SCHEMAS_DIR`, as `(<name>, <text>)`. `build.rs` embeds
/// every `*.toml` under `schemas/` as `TEMPLATE_TEXTS`, discovered,
/// its own stem paired with its text, so no file name is hand-kept
/// here. Nothing else reads this; a loaded project's schemas come only
/// from `load`, over the files `init` or the project's own author
/// wrote.
pub fn templates() -> Vec<(String, &'static str)> {
    TEMPLATE_TEXTS
        .iter()
        .map(|(stem, text)| (stem.to_string(), *text))
        .collect()
}

/// Where a project's schema files live, under the project root
/// `checkout_root` finds - what `load` reads and `init` writes.
pub const SCHEMAS_DIR: &str = ".percept/schemas";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaFile {
    purpose: String,
    #[serde(default)]
    nodes: IndexMap<String, IndexMap<String, PropertyValue>>,
    #[serde(default)]
    edges: IndexMap<String, EdgeFile>,
    #[serde(default)]
    rules: RulesFile,
}

/// The lines a schema injects into an agent's context, by moment - a
/// missing `[rules]` table means no rules at all. Any key parses here;
/// `parse` refuses one outside `MOMENTS`, so the "expected one of"
/// error is worded once rather than by serde's own unknown-field
/// message.
#[derive(Deserialize, Default)]
#[serde(transparent)]
struct RulesFile(IndexMap<String, Vec<String>>);

/// A node kind's property value as TOML writes it: a string declares
/// free text, its content unread; a list declares a closed set of
/// values.
#[derive(Deserialize)]
#[serde(untagged)]
enum PropertyValue {
    Text(String),
    Closed(Vec<String>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeFile {
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

/// The one key reserved in a node kind's table - its short id prefix.
/// Every other key names a property.
const PREFIX_KEY: &str = "prefix";

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
/// error, so a project with several files knows which one is wrong,
/// and names the schema itself - a file no longer declares its own
/// name.
fn parse(stem: &str, text: &str) -> Result<Schema, Box<dyn std::error::Error>> {
    let file: SchemaFile = toml::from_str(text).map_err(|err| format!("{stem}.toml: {err}"))?;

    if file.nodes.is_empty() {
        return Err(format!("{stem}.toml: declares no node kinds").into());
    }

    check_kind_names(stem, "node", file.nodes.keys())?;
    check_kind_names(stem, "edge", file.edges.keys())?;
    for (moment, lines) in &file.rules.0 {
        if !MOMENTS.contains(&moment.as_str()) {
            return Err(format!(
                "{stem}.toml: rules declares {moment:?}, which is not one of {}",
                MOMENTS.join(", ")
            )
            .into());
        }
        if lines.iter().any(|line| line.trim().is_empty()) {
            return Err(format!("{stem}.toml: rules declares a blank {moment:?} entry").into());
        }
    }

    let node_kinds: Vec<NodeKind> = file
        .nodes
        .into_iter()
        .map(|(kind, table)| node_kind(stem, kind, table))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    check_prefixes(stem, &node_kinds)?;

    let edge_kinds: Vec<EdgeKind> = file
        .edges
        .into_iter()
        .map(|(kind, edge)| {
            check_edge_end(stem, &kind, "from", &edge.from, &node_kinds)?;
            check_edge_end(stem, &kind, "to", &edge.to, &node_kinds)?;
            Ok(EdgeKind {
                kind,
                from: edge.from,
                to: edge.to,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    Ok(Schema {
        name: stem.to_string(),
        purpose: file.purpose,
        node_kinds,
        edge_kinds,
        rules: Rules::new(file.rules.0.into_iter().collect()),
    })
}

/// Builds one node kind from its declared table: `prefix`, when given,
/// pulled out first, every remaining key a property in declared order.
fn node_kind(
    stem: &str,
    kind: String,
    mut table: IndexMap<String, PropertyValue>,
) -> Result<NodeKind, Box<dyn std::error::Error>> {
    let prefix = match table.shift_remove(PREFIX_KEY) {
        Some(PropertyValue::Text(prefix)) => prefix,
        Some(PropertyValue::Closed(_)) => {
            return Err(format!(
                "{stem}.toml: node kind {kind:?} declares \"prefix\" as a list; prefix must be a \
                 string, not a property"
            )
            .into());
        }
        None => default_prefix(&kind),
    };

    let mut properties: Vec<(String, Vec<String>)> = Vec::with_capacity(table.len());
    let mut closed_already: Option<String> = None;
    for (name, value) in table {
        if name.trim().is_empty() {
            return Err(format!("{stem}.toml: node kind {kind:?} declares a blank property").into());
        }
        let values = match value {
            PropertyValue::Text(_) => Vec::new(),
            PropertyValue::Closed(values) => {
                if let Some(first) = &closed_already {
                    return Err(format!(
                        "{stem}.toml: node kind {kind:?} declares a second closed list, \
                         {name:?}; a kind carries at most one, alongside {first:?}"
                    )
                    .into());
                }
                if values.len() < 2 {
                    return Err(format!(
                        "{stem}.toml: node kind {kind:?} declares fewer than two values for {name:?}"
                    )
                    .into());
                }
                if values.iter().any(|value| value.trim().is_empty()) {
                    return Err(format!(
                        "{stem}.toml: node kind {kind:?} declares a blank value for {name:?}"
                    )
                    .into());
                }
                if let Some(value) = repeated(values.iter().map(String::as_str)) {
                    return Err(format!(
                        "{stem}.toml: node kind {kind:?} declares the value {value:?} twice for \
                         {name:?}"
                    )
                    .into());
                }
                closed_already = Some(name.clone());
                values
            }
        };
        properties.push((name, values));
    }

    Ok(NodeKind {
        prefix,
        kind,
        properties,
    })
}

/// Refuses a blank kind name among `names` - `group` names the kind
/// (`node` or `edge`) in the error. A repeated name cannot reach here:
/// `[nodes.x]` or `[edges.x]` written twice is a TOML parse error
/// before this point.
fn check_kind_names<'a>(
    stem: &str,
    group: &str,
    names: impl Iterator<Item = &'a String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for name in names {
        if name.trim().is_empty() {
            return Err(format!("{stem}.toml: a {group} kind must not be blank").into());
        }
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
/// every kind check shares.
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
