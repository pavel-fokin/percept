//! The code map: a static graph of a codebase, walked from the working
//! tree with `ignore` and parsed with `tree-sitter`, rather than folded
//! from the event log. `build` returns a `Map` on the same `Schema` and
//! `Map::apply` every log-backed map uses, so `maps list` and `maps
//! show` treat it the same way once it's built - `Map::fold_all` and
//! `SCHEMAS` stay log-only, since nothing here is an event. A language
//! adds one query file and one small module like `rust`; `build` is
//! where they're dispatched.

mod rust;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::path::Path;

use ignore::WalkBuilder;

use crate::percept::{Map, MapError, Mutation, NodeRef, CODE};

/// Why the code map couldn't be built: the walk or a read hit an I/O
/// error, or - this should not happen, since every kind here is one
/// `CODE` declares - a mutation broke the schema.
#[derive(Debug)]
pub enum Error {
    Walk(ignore::Error),
    Io(std::io::Error),
    Map(MapError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Walk(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Map(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ignore::Error> for Error {
    fn from(e: ignore::Error) -> Self {
        Self::Walk(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<MapError> for Error {
    fn from(e: MapError) -> Self {
        Self::Map(e)
    }
}

/// Builds the code map from every `.rs` file under `root`, gitignore
/// rules applied the way `ignore` applies them for any tool. Node ids
/// are minted fresh by `Map::apply`; nothing here keeps them, and
/// nothing here opens the event log.
pub fn build(root: &Path) -> Result<Map, Error> {
    let files = rust_files(root)?;
    let known: HashSet<String> = files.iter().cloned().collect();

    let mut map = Map::empty(&CODE);
    for file in &files {
        map.apply(Mutation::AddNode {
            kind: "file".to_string(),
            name: file.clone(),
            properties: BTreeMap::from([("language".to_string(), "rust".to_string())]),
            sources: Vec::new(),
        })?;
    }

    for file in &files {
        let source = std::fs::read_to_string(root.join(file))?;
        let Some((imports, symbols)) = rust::read(&source) else {
            continue;
        };

        for symbol in symbols {
            let name = format!("{file}::{}", symbol.path.join("::"));
            let properties = BTreeMap::from([
                ("public".to_string(), symbol.public.to_string()),
                ("line".to_string(), symbol.line.to_string()),
            ]);
            match map.apply(Mutation::AddNode {
                kind: symbol.kind.to_string(),
                name: name.clone(),
                properties,
                sources: Vec::new(),
            }) {
                Ok(_) => {}
                Err(MapError::DuplicateNode { .. }) => continue,
                Err(e) => return Err(e.into()),
            }
            map.apply(Mutation::AddEdge {
                kind: "contains".to_string(),
                from: NodeRef {
                    kind: "file".to_string(),
                    name: file.clone(),
                },
                to: NodeRef {
                    kind: symbol.kind.to_string(),
                    name,
                },
                sources: Vec::new(),
            })?;
        }

        let mut targets = BTreeSet::new();
        for path in &imports.paths {
            if let Some(target) = rust::resolve_path(file, path, &imports.modules, &known) {
                targets.insert(name_of(target));
            }
        }
        for module in &imports.modules {
            if let Some(target) = rust::resolve_module(file, module, &known) {
                targets.insert(name_of(target));
            }
        }

        for (kind, name) in targets {
            if kind == "package" && map.find(&kind, &name).is_none() {
                map.apply(Mutation::AddNode {
                    kind: "package".to_string(),
                    name: name.clone(),
                    properties: BTreeMap::new(),
                    sources: Vec::new(),
                })?;
            }
            map.apply(Mutation::AddEdge {
                kind: "imports".to_string(),
                from: NodeRef {
                    kind: "file".to_string(),
                    name: file.clone(),
                },
                to: NodeRef { kind, name },
                sources: Vec::new(),
            })?;
        }
    }

    Ok(map)
}

/// `target`'s node kind and name, so a batch of them can be
/// deduplicated by both before minting a package node or an edge.
fn name_of(target: rust::Target) -> (String, String) {
    match target {
        rust::Target::File(name) => ("file".to_string(), name),
        rust::Target::Package(name) => ("package".to_string(), name),
    }
}

/// Every `.rs` file under `root`, gitignore-aware, as paths relative to
/// `root` with `/` separators - the form a `file` node is named by.
fn rust_files(root: &Path) -> Result<Vec<String>, Error> {
    let mut files = Vec::new();
    for entry in WalkBuilder::new(root).build() {
        let entry = entry?;
        let is_rust_file = entry.file_type().is_some_and(|t| t.is_file())
            && entry.path().extension().is_some_and(|ext| ext == "rs");
        if !is_rust_file {
            continue;
        }
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        files.push(to_slash(relative));
    }
    files.sort();
    Ok(files)
}

fn to_slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests;
