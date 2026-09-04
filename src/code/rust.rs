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

/// What one query match captured of a symbol, before it is a `Symbol`.
#[derive(Default)]
struct Captured<'a> {
    def: Option<Node<'a>>,
    name: Option<Node<'a>>,
    public: bool,
    impl_type: Option<Node<'a>>,
    impl_trait: Option<Node<'a>>,
}

/// Parses `source` once and runs the query once over it, reading both
/// what the file imports and the symbols it defines, in source order.
/// A file with syntax errors still parses, around error nodes; `None`
/// only when tree-sitter gives up on the parse itself. Two symbols
/// with one name - `cfg`-gated twins - keep the first: a map names a
/// node once.
pub fn read(source: &str) -> Option<(Imports, Vec<Symbol>)> {
    let tree = parse(source)?;
    let query = query();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    let names = query.capture_names();

    let mut imports = Imports::default();
    let mut symbols: Vec<Symbol> = Vec::new();
    let mut seen = HashSet::new();
    while let Some(m) = matches.next() {
        let mut captured = Captured::default();
        for capture in m.captures() {
            let node = capture.node;
            match names[capture.index as usize] {
                "import.source" => imports.paths.extend(expand(node, source)),
                "module.decl" => imports.modules.push(text(node, source)),
                "symbol.def" => captured.def = Some(node),
                "symbol.name" => captured.name = Some(node),
                "symbol.visibility" => captured.public = true,
                "impl.type" => captured.impl_type = Some(node),
                "impl.trait" => captured.impl_trait = Some(node),
                _ => {}
            }
        }
        let (Some(def), Some(name)) = (captured.def, captured.name) else {
            continue;
        };
        let kind = if def.kind() == "function_item" {
            "function"
        } else {
            "type"
        };
        let path: Vec<String> = captured
            .impl_type
            .map(|node| base_name(node, source))
            .into_iter()
            .chain(captured.impl_trait.map(|node| trait_name(node, source)))
            .chain([text(name, source)])
            .collect();
        if seen.insert((kind, path.clone())) {
            symbols.push(Symbol {
                kind,
                path,
                public: captured.public,
                line: def.start_position().row + 1,
            });
        }
    }
    symbols.sort_by_key(|symbol| symbol.line);
    Some((imports, symbols))
}

/// A type as written on an `impl` block, with its generics, path, and
/// reference dropped: `Id<T>` is `Id`, `fmt::Display` is `Display`,
/// `&Foo` is `Foo`.
fn base_name(node: Node, source: &str) -> String {
    match node.kind() {
        "generic_type" => node
            .child_by_field_name("type")
            .map_or_else(String::new, |inner| base_name(inner, source)),
        "scoped_type_identifier" | "scoped_identifier" => node
            .child_by_field_name("name")
            .map_or_else(String::new, |inner| text(inner, source)),
        "reference_type" => node
            .child_by_field_name("type")
            .map_or_else(String::new, |inner| base_name(inner, source)),
        _ => text(node, source),
    }
}

/// A trait as written on an `impl` block, with its path dropped but
/// its arguments kept: `fmt::Display` is `Display`, `From<io::Error>`
/// stays as it is, because `impl From<A> for T` and `impl From<B> for
/// T` are two impls whose methods must not share a name.
fn trait_name(node: Node, source: &str) -> String {
    match (node.kind(), node.child_by_field_name("type_arguments")) {
        ("generic_type", Some(arguments)) => {
            format!("{}{}", base_name(node, source), text(arguments, source))
        }
        _ => base_name(node, source),
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
        "use_as_clause" => prefix_paths(node, source),
        // `use a::b::*` - the grammar gives the wildcard's path no
        // field name, so it is the first named child.
        "use_wildcard" => match node.named_child(0) {
            Some(path) => expand(path, source),
            None => vec![Vec::new()],
        },
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
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
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
        _ if declared.contains(head) => {
            resolve_beside(&own_module_dir(file), path, known).map(Target::File)
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
/// item rather than a module is what the shorter prefixes are for;
/// when no segment names a file the item lives in `base`'s own module,
/// so the answer is the file that speaks for it.
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
    module_file(base, known)
}

/// The file that speaks for the module whose children live in `base`:
/// `base/mod.rs`, `base.rs`, or for the crate root `base/main.rs` or
/// `base/lib.rs`.
fn module_file(base: &str, known: &HashSet<String>) -> Option<String> {
    [
        format!("{base}/mod.rs"),
        format!("{base}.rs"),
        format!("{base}/main.rs"),
        format!("{base}/lib.rs"),
    ]
    .into_iter()
    .find(|file| known.contains(file))
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
    }
}

#[cfg(test)]
mod tests;
