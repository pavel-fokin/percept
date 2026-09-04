//! Rust support for the code map: a tree-sitter query pulls every `use`
//! path, `mod x;` declaration, function, and type out of one file's
//! source; the functions below turn each into the node or edge it
//! names. What a query cannot express - how an import path resolves to
//! a file, what a generic or trait impl's type is called - is this
//! module's job, and a second language adds one more like it.

use std::collections::HashSet;
use std::sync::OnceLock;

use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator, Tree};

const QUERY_SRC: &str = include_str!("rust.scm");

fn query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        Query::new(&tree_sitter_rust::LANGUAGE.into(), QUERY_SRC)
            .expect("rust.scm is valid tree-sitter query syntax")
    })
}

fn parse(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("tree-sitter-rust's language loads");
    parser.parse(source, None)
}

/// What one file's `use` and `mod` statements name, before any of it is
/// resolved to a file or a package. A `use` path is already expanded -
/// a group like `use a::{b, c};` is two entries, not one.
#[derive(Default)]
pub struct Imports {
    /// Each a full path's segments, first-to-last, e.g. `["crate",
    /// "store", "Jsonl"]` or `["std", "io", "Write"]`.
    pub paths: Vec<Vec<String>>,
    /// The name in each `mod x;` with no body - the declaration of a
    /// file the current one pulls in, not an inline module.
    pub modules: Vec<String>,
}

/// Parses `source` once and reads both its imports and its symbols -
/// the pair `code::build` wants per file, since the parse is the
/// expensive step and a file gets only one. A file with syntax errors
/// still parses, around error nodes; `None` only when tree-sitter
/// gives up on the parse itself.
pub fn read(source: &str) -> Option<(Imports, Vec<Symbol>)> {
    let tree = parse(source)?;
    let root = tree.root_node();
    Some((read_imports(root, source), read_symbols(root, source)))
}

fn read_imports(root: Node, source: &str) -> Imports {
    let query = query();
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(query, root, source.as_bytes());
    let names = query.capture_names();

    let mut result = Imports::default();
    while let Some((m, index)) = captures.next() {
        let capture = m.captures()[*index];
        match names[capture.index as usize] {
            "import.source" => result.paths.extend(expand(capture.node, source)),
            "module.decl" => result.modules.push(text(capture.node, source)),
            _ => {}
        }
    }
    result
}

/// One module-level function or type, or one method in a module-level
/// `impl` block, read from a file.
pub struct Symbol {
    pub kind: &'static str,
    /// The name's segments after the file, first-to-last: `["map_of"]`
    /// for a free function, `["Map", "apply"]` for an inherent method,
    /// `["Node", "Display", "fmt"]` for a trait impl method.
    pub path: Vec<String>,
    pub public: bool,
    pub line: usize,
}

fn read_symbols(root: Node, source: &str) -> Vec<Symbol> {
    let query = query();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, source.as_bytes());
    let names = query.capture_names();

    let mut symbols = Vec::new();
    while let Some(m) = matches.next() {
        let mut function_def = None;
        let mut function_name = None;
        let mut function_public = false;
        let mut type_def = None;
        let mut type_name = None;
        let mut type_public = false;
        let mut impl_type = None;
        let mut impl_trait = None;

        for capture in m.captures() {
            match names[capture.index as usize] {
                "function.def" => function_def = Some(capture.node),
                "function.name" => function_name = Some(capture.node),
                "function.visibility" => function_public = true,
                "type.def" => type_def = Some(capture.node),
                "type.name" => type_name = Some(capture.node),
                "type.visibility" => type_public = true,
                "impl.type" => impl_type = Some(capture.node),
                "impl.trait" => impl_trait = Some(capture.node),
                _ => {}
            }
        }

        if let (Some(def), Some(name)) = (function_def, function_name) {
            let mut path: Vec<String> = impl_type
                .into_iter()
                .chain(impl_trait)
                .map(|node| base_name(node, source))
                .collect();
            path.push(text(name, source));
            symbols.push(Symbol {
                kind: "function",
                path,
                public: function_public,
                line: def.start_position().row + 1,
            });
        } else if let (Some(def), Some(name)) = (type_def, type_name) {
            symbols.push(Symbol {
                kind: "type",
                path: vec![text(name, source)],
                public: type_public,
                line: def.start_position().row + 1,
            });
        }
    }

    symbols.sort_by_key(|symbol| symbol.line);
    symbols
}

/// A type or trait as written on an `impl` block, with its generics and
/// path dropped: `Id<T>` is `Id`, `fmt::Display` is `Display`.
fn base_name(node: Node, source: &str) -> String {
    match node.kind() {
        "generic_type" => node
            .child_by_field_name("type")
            .map_or_else(String::new, |inner| base_name(inner, source)),
        "scoped_type_identifier" | "scoped_identifier" => node
            .child_by_field_name("name")
            .map_or_else(String::new, |inner| text(inner, source)),
        _ => text(node, source),
    }
}

fn text(node: Node, source: &str) -> String {
    node.utf8_text(source.as_bytes())
        .expect("a node's range always falls on the source it was parsed from")
        .to_string()
}

/// Expands one `use` argument into every full path it names. A group -
/// `use_list`, `scoped_use_list` - recurses into each element and joins
/// it to its prefix, so `use crate::{a, b::c}` becomes two paths and
/// `use std::io::{self, Write}` becomes one path for the module itself
/// (`self` inside a group names the group's own prefix, not a further
/// segment) and one for `Write`.
fn expand(node: Node, source: &str) -> Vec<Vec<String>> {
    match node.kind() {
        "identifier" | "crate" | "self" | "super" => vec![vec![text(node, source)]],
        "scoped_identifier" => {
            let Some(name) = node.child_by_field_name("name") else {
                return Vec::new();
            };
            let prefixes = prefix_paths(node, source);
            let segment = text(name, source);
            prefixes
                .into_iter()
                .map(|mut path| {
                    path.push(segment.clone());
                    path
                })
                .collect()
        }
        "scoped_use_list" => {
            let Some(list) = node.child_by_field_name("list") else {
                return Vec::new();
            };
            let prefixes = prefix_paths(node, source);
            let suffixes: Vec<Vec<String>> = expand(list, source)
                .into_iter()
                .map(|suffix| {
                    if suffix == ["self"] {
                        Vec::new()
                    } else {
                        suffix
                    }
                })
                .collect();
            prefixes
                .iter()
                .flat_map(|prefix| {
                    suffixes.iter().map(move |suffix| {
                        let mut path = prefix.clone();
                        path.extend(suffix.iter().cloned());
                        path
                    })
                })
                .collect()
        }
        "use_list" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .flat_map(|child| expand(child, source))
                .collect()
        }
        "use_wildcard" | "use_as_clause" => prefix_paths(node, source),
        _ => Vec::new(),
    }
}

/// The paths named by a node's `path` field, expanded - or one empty
/// path when the field is absent, so a caller can append its own
/// segment or group onto it either way.
fn prefix_paths(node: Node, source: &str) -> Vec<Vec<String>> {
    match node.child_by_field_name("path") {
        Some(path) => expand(path, source),
        None => vec![Vec::new()],
    }
}

/// What a resolved import points at: another file in the walk, or a
/// package outside it.
pub enum Target {
    File(String),
    Package(String),
}

/// Resolves one `use` path named on `file` against `known` - the
/// relative paths of every file the walk found. `crate::` resolves
/// against the nearest ancestor `src` directory; `self::`, `super::`,
/// and a bare name `file` declares with `mod` - `declared` - resolve
/// from `file`'s own module; anything else names a package. `None`
/// when the path resolves to nothing the walk found - a module behind
/// a `cfg` the walk never parsed, or an item the search never finds a
/// file for.
pub fn resolve_path(
    file: &str,
    path: &[String],
    declared: &[String],
    known: &HashSet<String>,
) -> Option<Target> {
    let (head, rest) = path.split_first()?;
    match head.as_str() {
        "crate" => {
            let base = crate_root(file)?;
            resolve_beside(&base, rest, known).map(Target::File)
        }
        "self" => {
            let base = own_module_dir(file);
            resolve_beside(&base, rest, known).map(Target::File)
        }
        name if declared.contains(head) => {
            let base = own_module_dir(file);
            let segments: Vec<String> = std::iter::once(name.to_string())
                .chain(rest.iter().cloned())
                .collect();
            resolve_beside(&base, &segments, known).map(Target::File)
        }
        "super" => {
            let mut base = parent_module_dir(file)?;
            let mut rest = rest;
            while rest.first().map(String::as_str) == Some("super") {
                base = parent_dir(&base)?;
                rest = &rest[1..];
            }
            resolve_beside(&base, rest, known).map(Target::File)
        }
        other => Some(Target::Package(other.to_string())),
    }
}

/// Resolves `mod name;` on `file` against `known`, the way `self::name`
/// would.
pub fn resolve_module(file: &str, name: &str, known: &HashSet<String>) -> Option<Target> {
    let base = own_module_dir(file);
    let segment = [name.to_string()];
    resolve_beside(&base, &segment, known).map(Target::File)
}

/// The longest prefix of `segments`, joined onto `base`, that names a
/// file `known` holds - `base/a/b.rs` or `base/a/b/mod.rs` before
/// `base/a.rs` or `base/a/mod.rs`. A trailing segment that names an
/// item rather than a module is what the shorter prefixes are for.
fn resolve_beside(base: &str, segments: &[String], known: &HashSet<String>) -> Option<String> {
    for end in (1..=segments.len()).rev() {
        let joined = join(base, &segments[..end]);
        let as_file = format!("{joined}.rs");
        if known.contains(&as_file) {
            return Some(as_file);
        }
        let as_mod = format!("{joined}/mod.rs");
        if known.contains(&as_mod) {
            return Some(as_mod);
        }
    }
    None
}

fn join(base: &str, segments: &[String]) -> String {
    if base.is_empty() {
        segments.join("/")
    } else {
        format!("{base}/{}", segments.join("/"))
    }
}

fn split_dir_base(file: &str) -> (&str, &str) {
    file.rsplit_once('/').unwrap_or(("", file))
}

fn is_mod_style(base: &str) -> bool {
    matches!(base, "mod.rs" | "main.rs" | "lib.rs")
}

/// The directory `file`'s own `mod x;` declarations resolve against:
/// its own directory when it already speaks for one - `mod.rs`,
/// `main.rs`, `lib.rs` - or a sibling directory named after it
/// otherwise, since `src/foo.rs` and `src/foo/bar.rs` are one module.
fn own_module_dir(file: &str) -> String {
    let (dir, base) = split_dir_base(file);
    if is_mod_style(base) {
        dir.to_string()
    } else {
        let stem = [base.strip_suffix(".rs").unwrap_or(base).to_string()];
        join(dir, &stem)
    }
}

/// The directory the module that declares `file` would itself resolve
/// `mod` declarations against - `None` for `main.rs`/`lib.rs`, which
/// name the crate root and so have no parent module.
fn parent_module_dir(file: &str) -> Option<String> {
    let (dir, base) = split_dir_base(file);
    match base {
        "main.rs" | "lib.rs" => None,
        "mod.rs" => parent_dir(dir),
        _ => Some(dir.to_string()),
    }
}

fn parent_dir(dir: &str) -> Option<String> {
    match dir.rsplit_once('/') {
        Some((parent, _)) => Some(parent.to_string()),
        None if dir.is_empty() => None,
        None => Some(String::new()),
    }
}

/// The nearest ancestor of `file` named `src`, `crate::` paths' base -
/// `None` if `file` sits outside any directory named `src`.
fn crate_root(file: &str) -> Option<String> {
    let mut dir = split_dir_base(file).0.to_string();
    loop {
        if dir.rsplit('/').next() == Some("src") {
            return Some(dir);
        }
        dir = parent_dir(&dir)?;
        if dir.is_empty() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests;
