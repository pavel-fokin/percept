//! The code map: a static graph of a codebase, walked from the working
//! tree with `ignore` and parsed with `tree-sitter`, rather than folded
//! from the event log. `build` returns a `Map` on the same `Schema` and
//! `Map::apply` every log-backed map uses, so `maps list` and `maps
//! show` treat it the same way once it's built - `Map::fold_all` and
//! `SCHEMAS` stay log-only, since nothing here is an event. A language
//! adds one query file and one small module like `rust`; `build` is
//! where they're dispatched.

mod rust;

use rust::Target;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use ignore::WalkBuilder;

use crate::percept::{Map, MapError, Mutation, NodeRef, CODE};

/// Builds the code map from every `.rs` file under `root`, gitignore
/// rules applied the way `ignore` applies them for any tool. Node ids
/// are minted fresh by `Map::apply`; nothing here keeps them, and
/// nothing here opens the event log. The only error is a mutation the
/// schema refuses, which every kind here being one `CODE` declares
/// should make impossible.
pub fn build(root: &Path) -> Result<Map, MapError> {
    let files = rust_files(root);
    let known: HashSet<String> = files.iter().cloned().collect();

    let mut map = Map::empty(&CODE);
    let mut packages = HashSet::new();
    for file in &files {
        map.apply(Mutation::AddNode {
            kind: "file".to_string(),
            name: file.clone(),
            properties: BTreeMap::from([("language".to_string(), "rust".to_string())]),
            sources: Vec::new(),
        })?;
    }

    for file in &files {
        // A file that can't be read is not a fact of the map. Say so
        // and carry on: one bad file must not cost the other hundred.
        let source = match std::fs::read(root.join(file)) {
            Ok(bytes) => String::from_utf8(bytes)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
            Err(e) => {
                eprintln!("percept: skipping {file}: {e}");
                continue;
            }
        };
        let Some((imports, symbols)) = rust::read(&source) else {
            continue;
        };

        let this = NodeRef {
            kind: "file".to_string(),
            name: file.clone(),
        };

        for symbol in symbols {
            let node = NodeRef {
                kind: symbol.kind.to_string(),
                name: format!("{file}::{}", symbol.path.join("::")),
            };
            map.apply(Mutation::AddNode {
                kind: node.kind.clone(),
                name: node.name.clone(),
                properties: BTreeMap::from([
                    ("public".to_string(), symbol.public.to_string()),
                    ("line".to_string(), symbol.line.to_string()),
                ]),
                sources: Vec::new(),
            })?;
            map.apply(Mutation::AddEdge {
                kind: "contains".to_string(),
                from: this.clone(),
                to: node,
                sources: Vec::new(),
            })?;
        }

        let paths = imports
            .paths
            .iter()
            .filter_map(|path| rust::resolve_path(file, path, &imports.modules, &known));
        let modules = imports
            .modules
            .iter()
            .filter_map(|module| rust::resolve_module(file, module, &known));
        let targets: BTreeSet<Target> = paths.chain(modules).collect();

        for target in targets {
            let to = match target {
                // `use self::Item` names the file itself: no edge, a
                // file does not import itself.
                Target::File(name) if name == *file => continue,
                Target::File(name) => NodeRef {
                    kind: "file".to_string(),
                    name,
                },
                Target::Package(name) => {
                    if packages.insert(name.clone()) {
                        map.apply(Mutation::AddNode {
                            kind: "package".to_string(),
                            name: name.clone(),
                            properties: BTreeMap::new(),
                            sources: Vec::new(),
                        })?;
                    }
                    NodeRef {
                        kind: "package".to_string(),
                        name,
                    }
                }
            };
            map.apply(Mutation::AddEdge {
                kind: "imports".to_string(),
                from: this.clone(),
                to,
                sources: Vec::new(),
            })?;
        }
    }

    Ok(map)
}

/// Every `.rs` file under `root`, gitignore-aware whether or not `root`
/// is a git checkout - an exported tree keeps its `.gitignore` and its
/// `target/` - as paths relative to `root` with `/` separators, the
/// form a `file` node is named by. An entry the walk can't open is
/// reported and skipped, not the end of the map.
fn rust_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for entry in WalkBuilder::new(root).require_git(false).build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("percept: skipping: {e}");
                continue;
            }
        };
        let is_rust_file = entry.file_type().is_some_and(|t| t.is_file())
            && entry.path().extension().is_some_and(|ext| ext == "rs");
        if !is_rust_file {
            continue;
        }
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        files.push(to_slash(relative));
    }
    files.sort();
    files
}

fn to_slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests;
