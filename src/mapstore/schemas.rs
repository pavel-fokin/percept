//! Loads a project's cognitive-map schemas from
//! `<project>/.percept/schemas/*.toml`. `core` stays serde-free, so
//! the parsing lives here. `core` checks the resulting declaration. A
//! project with no such directory, or none in it, declares no maps at
//! all: `load` returns an empty `SchemaCatalog`, and `percept init
//! <client>` is what gives a fresh checkout its first schema file,
//! copied from the templates this binary embeds - see `templates`.

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};

use crate::core::{EdgeKind, NodeKind, Schema, Schemas};

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
}

/// A node kind's property value as TOML writes it: a string declares
/// free text and its content is never read, so it must be `""` -
/// anything else is refused rather than silently dropped; a list
/// declares a closed set of values.
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

/// The `Schemas` port's concrete implementor: every schema
/// `<project>/.percept/schemas` declares, folded once by `load`.
pub struct SchemaCatalog {
    schemas: Vec<Arc<Schema>>,
}

impl SchemaCatalog {
    /// `folded`, each wrapped in `Arc` - the one full set every caller
    /// builds from.
    pub fn new(folded: Vec<Schema>) -> Self {
        let schemas: Vec<Arc<Schema>> = folded.into_iter().map(Arc::new).collect();
        Self { schemas }
    }
}

impl Schemas for SchemaCatalog {
    fn folded(&self) -> &[Arc<Schema>] {
        &self.schemas
    }
}

/// Every schema `<project>/.percept/schemas` declares, in the stable
/// order `project_files` gives - empty when the directory is missing
/// or holds no `.toml` file, so a project that has not run `percept
/// init <client>` yet has no maps at all. Each error names the file it
/// came from.
pub fn load(project: &Path) -> Result<SchemaCatalog, Box<dyn std::error::Error>> {
    let folded = project_files(project)?
        .into_iter()
        .map(|(stem, text)| parse(&stem, &text))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SchemaCatalog::new(folded))
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

    let node_kinds: Vec<NodeKind> = file
        .nodes
        .into_iter()
        .map(|(kind, table)| node_kind(stem, kind, table))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    let edge_kinds: Vec<EdgeKind> = file
        .edges
        .into_iter()
        .map(|(kind, edge)| {
            EdgeKind::new(kind, edge.from, edge.to)
                .map_err(|err| format!("{stem}.toml: {err}").into())
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    Schema::new(stem.to_string(), file.purpose, node_kinds, edge_kinds)
        .map_err(|err| format!("{stem}.toml: {err}").into())
}

/// Builds one node kind from its declared table: `prefix`, when given,
/// pulled out first, every remaining key a property in declared order.
fn node_kind(
    stem: &str,
    kind: String,
    mut table: IndexMap<String, PropertyValue>,
) -> Result<NodeKind, Box<dyn std::error::Error>> {
    let prefix = match table.shift_remove(PREFIX_KEY) {
        Some(PropertyValue::Text(prefix)) => Some(prefix),
        Some(PropertyValue::Closed(_)) => {
            return Err(format!(
                "{stem}.toml: node kind {kind:?} declares \"prefix\" as a list; prefix must be a \
                 string, not a property"
            )
            .into());
        }
        None => None,
    };

    let mut properties: Vec<(String, Vec<String>)> = Vec::with_capacity(table.len());
    for (name, value) in table {
        let values = match value {
            PropertyValue::Text(text) => {
                if !text.is_empty() {
                    return Err(format!(
                        "{stem}.toml: node kind {kind:?} declares free property {name:?} as \
                         {text:?}; a free property's value is always \"\", since its content is \
                         never read"
                    )
                    .into());
                }
                Vec::new()
            }
            PropertyValue::Closed(values) => values,
        };
        properties.push((name, values));
    }

    match prefix {
        Some(prefix) => NodeKind::with_prefix(kind, prefix, properties),
        None => NodeKind::new(kind, properties),
    }
    .map_err(|err| format!("{stem}.toml: {err}").into())
}

#[cfg(test)]
mod tests;
