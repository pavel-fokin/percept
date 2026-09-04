use super::*;
use std::collections::HashSet;

fn imports(source: &str) -> Option<Imports> {
    let tree = parse(source)?;
    Some(read_imports(tree.root_node(), source))
}

fn symbols(source: &str) -> Option<Vec<Symbol>> {
    let tree = parse(source)?;
    Some(read_symbols(tree.root_node(), source))
}

fn known(files: &[&str]) -> HashSet<String> {
    files.iter().map(|f| f.to_string()).collect()
}

fn path(segments: &[&str]) -> Vec<String> {
    segments.iter().map(|s| s.to_string()).collect()
}

fn target_name(target: Target) -> String {
    match target {
        Target::File(name) => name,
        Target::Package(name) => name,
    }
}

#[test]
fn a_use_group_expands_to_every_member() {
    let source = "use std::collections::{BTreeMap, HashSet};";
    let imports = imports(source).unwrap();
    assert_eq!(
        imports.paths,
        vec![
            path(&["std", "collections", "BTreeMap"]),
            path(&["std", "collections", "HashSet"]),
        ]
    );
}

#[test]
fn a_self_member_of_a_group_names_the_group_prefix_alone() {
    let source = "use std::io::{self, Write};";
    let imports = imports(source).unwrap();
    assert_eq!(
        imports.paths,
        vec![path(&["std", "io"]), path(&["std", "io", "Write"])]
    );
}

#[test]
fn a_module_declaration_with_no_body_is_captured() {
    let source = "mod foo;\nmod bar { }\n";
    let imports = imports(source).unwrap();
    assert_eq!(imports.modules, vec!["foo".to_string()]);
}

#[test]
fn crate_path_resolves_to_the_longest_existing_prefix() {
    let known = known(&["src/main.rs", "src/percept/map.rs", "src/percept/mod.rs"]);
    let target = resolve_path(
        "src/main.rs",
        &path(&["crate", "percept", "map", "Map"]),
        &[],
        &known,
    )
    .unwrap();
    assert_eq!(target_name(target), "src/percept/map.rs");
}

#[test]
fn crate_path_resolves_to_a_mod_rs_directory() {
    let known = known(&["src/main.rs", "src/store/mod.rs", "src/store/event.rs"]);
    let target = resolve_path(
        "src/main.rs",
        &path(&["crate", "store", "Jsonl"]),
        &[],
        &known,
    )
    .unwrap();
    assert_eq!(target_name(target), "src/store/mod.rs");
}

#[test]
fn mod_x_resolves_beside_the_file() {
    let known = known(&["src/main.rs", "src/app.rs"]);
    let target = resolve_module("src/main.rs", "app", &known).unwrap();
    assert_eq!(target_name(target), "src/app.rs");
}

#[test]
fn mod_x_resolves_to_a_directory_beside_a_mod_rs() {
    let known = known(&["src/foo/mod.rs", "src/foo/bar/mod.rs"]);
    let target = resolve_module("src/foo/mod.rs", "bar", &known).unwrap();
    assert_eq!(target_name(target), "src/foo/bar/mod.rs");
}

#[test]
fn mod_x_resolves_to_a_directory_beside_a_plain_file() {
    let known = known(&["src/percept/map.rs", "src/percept/map/tests.rs"]);
    let target = resolve_module("src/percept/map.rs", "tests", &known).unwrap();
    assert_eq!(target_name(target), "src/percept/map/tests.rs");
}

#[test]
fn super_resolves_from_the_parent_module() {
    let unresolved = known(&["src/percept/map.rs"]);
    let target = resolve_path(
        "src/percept/map.rs",
        &path(&["super", "event"]),
        &[],
        &unresolved,
    );
    assert!(target.is_none(), "no percept/event.rs in the known set");

    let sibling = known(&["src/percept/map.rs", "src/percept/event.rs"]);
    let target = resolve_path(
        "src/percept/map.rs",
        &path(&["super", "event"]),
        &[],
        &sibling,
    )
    .unwrap();
    assert_eq!(target_name(target), "src/percept/event.rs");
}

#[test]
fn an_external_crate_becomes_a_package() {
    let known = known(&["src/main.rs"]);
    let target = resolve_path("src/main.rs", &path(&["clap", "Parser"]), &[], &known).unwrap();
    assert!(matches!(target, Target::Package(name) if name == "clap"));
}

#[test]
fn an_unresolved_crate_path_is_none() {
    let known = known(&["src/main.rs"]);
    assert!(resolve_path("src/main.rs", &path(&["crate", "nope"]), &[], &known).is_none());
}

#[test]
fn a_bare_name_the_file_declares_as_a_module_resolves_beside_it() {
    let known = known(&["src/main.rs", "src/app/mod.rs"]);
    let declared = path(&["app"]);
    let target = resolve_path("src/main.rs", &path(&["app", "App"]), &declared, &known).unwrap();
    assert_eq!(target_name(target), "src/app/mod.rs");

    let undeclared = resolve_path("src/main.rs", &path(&["app", "App"]), &[], &known).unwrap();
    assert!(matches!(undeclared, Target::Package(name) if name == "app"));
}

fn function_paths(symbols: &[Symbol]) -> Vec<String> {
    symbols
        .iter()
        .filter(|s| s.kind == "function")
        .map(|s| s.path.join("::"))
        .collect()
}

fn type_paths(symbols: &[Symbol]) -> Vec<String> {
    symbols
        .iter()
        .filter(|s| s.kind == "type")
        .map(|s| s.path.join("::"))
        .collect()
}

#[test]
fn a_free_function_is_a_symbol_named_by_itself() {
    let symbols = symbols("fn greet() {}").unwrap();
    assert_eq!(function_paths(&symbols), vec!["greet"]);
}

#[test]
fn a_pub_function_is_public_and_a_private_one_is_not() {
    let symbols = symbols("pub fn greet() {}\nfn helper() {}").unwrap();
    let public: Vec<bool> = symbols.iter().map(|s| s.public).collect();
    assert_eq!(public, vec![true, false]);
}

#[test]
fn a_pub_crate_function_is_public() {
    let symbols = symbols("pub(crate) fn greet() {}").unwrap();
    assert!(symbols[0].public);
}

#[test]
fn an_inherent_method_is_qualified_by_its_type() {
    let source = "struct Map;\nimpl Map {\n    fn apply(&self) {}\n}\n";
    let symbols = symbols(source).unwrap();
    assert_eq!(function_paths(&symbols), vec!["Map::apply"]);
}

#[test]
fn a_trait_impl_method_is_qualified_by_its_type_and_trait() {
    let source = "struct Node;\nimpl std::fmt::Display for Node {\n    fn fmt(&self) {}\n}\n";
    let symbols = symbols(source).unwrap();
    assert_eq!(function_paths(&symbols), vec!["Node::Display::fmt"]);
}

#[test]
fn a_generic_impl_drops_the_type_parameter() {
    let source = "struct Id<T>(T);\nimpl<T> Clone for Id<T> {\n    fn clone(&self) -> Self {}\n}\n";
    let symbols = symbols(source).unwrap();
    assert_eq!(function_paths(&symbols), vec!["Id::Clone::clone"]);
}

#[test]
fn struct_enum_trait_and_alias_are_type_symbols() {
    let source = "struct A;\nenum B {}\ntrait C {}\ntype D = A;\n";
    let symbols = symbols(source).unwrap();
    assert_eq!(type_paths(&symbols), vec!["A", "B", "C", "D"]);
}

#[test]
fn contents_of_an_inline_module_are_skipped() {
    let source = "mod tests {\n    fn helper() {}\n    struct Fixture;\n}\n";
    let symbols = symbols(source).unwrap();
    assert!(symbols.is_empty());
}

#[test]
fn a_function_nested_in_a_function_body_is_skipped() {
    let source = "fn outer() {\n    fn inner() {}\n}\n";
    let symbols = symbols(source).unwrap();
    assert_eq!(function_paths(&symbols), vec!["outer"]);
}

#[test]
fn a_symbols_line_is_the_items_first_token() {
    let source = "\n\npub fn greet() {}\n";
    let symbols = symbols(source).unwrap();
    assert_eq!(symbols[0].line, 3);
}
