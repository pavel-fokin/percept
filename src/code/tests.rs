use super::*;
use crate::core::testing::Fixture;

fn build_map(fixture: &Fixture) -> Map {
    build(fixture.path()).unwrap()
}

fn has_edge(map: &Map, from: (&str, &str), kind: &str, to: (&str, &str)) -> bool {
    let from = map.find(from.0, from.1).unwrap().id;
    let to = map.find(to.0, to.1).unwrap().id;
    map.edges()
        .iter()
        .any(|edge| edge.kind == kind && edge.from == from && edge.to == to)
}

#[test]
fn mod_x_resolves_to_x_rs() {
    let fixture = Fixture::new();
    fixture
        .write("src/main.rs", "mod app;\n")
        .write("src/app.rs", "");

    let map = build_map(&fixture);

    assert!(has_edge(
        &map,
        ("file", "src/main.rs"),
        "imports",
        ("file", "src/app.rs"),
    ));
}

#[test]
fn mod_x_resolves_to_x_mod_rs() {
    let fixture = Fixture::new();
    fixture
        .write("src/main.rs", "mod store;\n")
        .write("src/store/mod.rs", "");

    let map = build_map(&fixture);

    assert!(has_edge(
        &map,
        ("file", "src/main.rs"),
        "imports",
        ("file", "src/store/mod.rs"),
    ));
}

#[test]
fn use_crate_resolves_to_the_longest_existing_prefix() {
    let fixture = Fixture::new();
    fixture
        .write("src/main.rs", "use crate::percept::map::Map;\n")
        .write("src/percept/mod.rs", "")
        .write("src/percept/map.rs", "");

    let map = build_map(&fixture);

    assert!(has_edge(
        &map,
        ("file", "src/main.rs"),
        "imports",
        ("file", "src/percept/map.rs"),
    ));
}

#[test]
fn super_resolves_from_the_parent_module() {
    let fixture = Fixture::new();
    fixture
        .write("src/percept/mod.rs", "mod event;\nmod map;\n")
        .write("src/percept/event.rs", "")
        .write("src/percept/map.rs", "use super::event::EventId;\n");

    let map = build_map(&fixture);

    assert!(has_edge(
        &map,
        ("file", "src/percept/map.rs"),
        "imports",
        ("file", "src/percept/event.rs"),
    ));
}

#[test]
fn a_use_group_expands_to_an_edge_per_member() {
    let fixture = Fixture::new();
    fixture.write(
        "src/main.rs",
        "use std::collections::{BTreeMap, HashSet};\n",
    );

    let map = build_map(&fixture);

    assert!(has_edge(
        &map,
        ("file", "src/main.rs"),
        "imports",
        ("package", "std"),
    ));
}

#[test]
fn two_files_importing_the_same_crate_share_one_package_node() {
    let fixture = Fixture::new();
    fixture
        .write("src/main.rs", "use clap::Parser;\n")
        .write("src/cli.rs", "use clap::Args;\n");

    let map = build_map(&fixture);

    let packages: Vec<_> = map
        .nodes()
        .iter()
        .filter(|node| node.kind == "package" && node.name == "clap")
        .collect();
    assert_eq!(packages.len(), 1);
    assert!(has_edge(
        &map,
        ("file", "src/main.rs"),
        "imports",
        ("package", "clap"),
    ));
    assert!(has_edge(
        &map,
        ("file", "src/cli.rs"),
        "imports",
        ("package", "clap"),
    ));
}

#[test]
fn two_uses_of_the_same_crate_in_one_file_collapse_to_one_edge() {
    let fixture = Fixture::new();
    fixture.write("src/main.rs", "use clap::Parser;\nuse clap::Args;\n");

    let map = build_map(&fixture);

    let edges: Vec<_> = map
        .edges()
        .iter()
        .filter(|edge| edge.kind == "imports")
        .collect();
    assert_eq!(edges.len(), 1);
}

#[test]
fn a_gitignored_file_is_skipped() {
    let fixture = Fixture::new();
    fixture
        .write(".gitignore", "generated.rs\n")
        .write("src/main.rs", "mod generated;\n")
        .write("src/generated.rs", "");

    let map = build_map(&fixture);

    assert!(map.find("file", "src/generated.rs").is_none());
    assert!(map.edges().is_empty());
}

#[test]
fn a_functions_symbol_carries_public_and_line_and_is_contained_by_its_file() {
    let fixture = Fixture::new();
    fixture.write("src/main.rs", "\npub fn greet() {}\n");

    let map = build_map(&fixture);

    let node = map.find("function", "src/main.rs::greet").unwrap();
    assert_eq!(
        node.properties.get("public").map(String::as_str),
        Some("true")
    );
    assert_eq!(node.properties.get("line").map(String::as_str), Some("2"));
    assert!(has_edge(
        &map,
        ("file", "src/main.rs"),
        "contains",
        ("function", "src/main.rs::greet"),
    ));
}

#[test]
fn a_cfg_gated_duplicate_name_skips_rather_than_fails() {
    let fixture = Fixture::new();
    fixture.write(
        "src/main.rs",
        "#[cfg(unix)]\nfn greet() {}\n#[cfg(windows)]\nfn greet() {}\n",
    );

    let map = build_map(&fixture);

    let symbols: Vec<_> = map
        .nodes()
        .iter()
        .filter(|node| node.kind == "function" && node.name == "src/main.rs::greet")
        .collect();
    assert_eq!(symbols.len(), 1);
}

#[test]
fn a_use_of_a_module_the_file_declares_is_a_file_edge_not_a_package() {
    let fixture = Fixture::new();
    fixture
        .write("src/main.rs", "mod app;\nuse app::App;\n")
        .write("src/app/mod.rs", "");

    let map = build_map(&fixture);

    assert!(map.find("package", "app").is_none());
    assert!(has_edge(
        &map,
        ("file", "src/main.rs"),
        "imports",
        ("file", "src/app/mod.rs"),
    ));
}

#[test]
fn a_gitignore_is_honoured_outside_a_git_checkout() {
    let fixture = Fixture::new();
    fixture
        .write(".gitignore", "target\n")
        .write("src/main.rs", "")
        .write("target/debug/build/gen.rs", "pub fn gen() {}\n");

    let map = build_map(&fixture);

    assert!(map.find("file", "target/debug/build/gen.rs").is_none());
}

#[test]
fn a_use_inside_an_inline_module_is_not_the_file_s_import() {
    let fixture = Fixture::new();
    fixture.write("src/helper.rs", "").write(
        "src/store/mod.rs",
        "#[cfg(test)]\nmod tests {\n    use super::helper::h;\n}\n",
    );

    let map = build_map(&fixture);

    assert!(map.edges().is_empty());
}

#[test]
fn a_use_of_the_file_s_own_item_makes_no_edge() {
    let fixture = Fixture::new();
    fixture.write("src/foo/mod.rs", "pub struct Bar;\nuse crate::foo::Bar;\n");

    let map = build_map(&fixture);

    assert!(map.edges().iter().all(|edge| edge.kind != "imports"));
}

#[test]
fn a_glob_import_of_the_parent_is_an_edge_to_the_parent_s_file() {
    let fixture = Fixture::new();
    fixture
        .write(
            "src/percept/map.rs",
            "pub struct Map;\n#[cfg(test)]\nmod tests;\n",
        )
        .write("src/percept/map/tests.rs", "use super::*;\n");

    let map = build_map(&fixture);

    assert!(has_edge(
        &map,
        ("file", "src/percept/map/tests.rs"),
        "imports",
        ("file", "src/percept/map.rs"),
    ));
}

#[test]
fn two_impls_of_one_generic_trait_keep_their_methods_apart() {
    let fixture = Fixture::new();
    fixture.write(
        "src/main.rs",
        "struct E;\nimpl From<A> for E { fn from(a: A) -> E { E } }\nimpl From<B> for E { fn from(b: B) -> E { E } }\n",
    );

    let map = build_map(&fixture);

    assert!(map
        .find("function", "src/main.rs::E::From<A>::from")
        .is_some());
    assert!(map
        .find("function", "src/main.rs::E::From<B>::from")
        .is_some());
}

#[test]
fn every_kind_of_the_schema_carries_a_gloss() {
    let s = schema();
    for kind in &s.node_kinds {
        assert!(!kind.gloss.is_empty(), "kind {:?} has no gloss", kind.kind);
    }
    for kind in &s.edge_kinds {
        assert!(!kind.gloss.is_empty(), "kind {:?} has no gloss", kind.kind);
    }
}

#[test]
fn the_package_gloss_says_it_is_an_external_crate() {
    let s = schema();
    let package = s.node_kind("package").unwrap();
    assert!(package.gloss.contains("external crate"));
    assert!(package
        .gloss
        .contains("never one of this project's own modules"));
}

#[test]
fn the_schema_s_node_kinds_carry_distinct_prefixes() {
    let s = schema();
    let mut prefixes: Vec<&str> = s.node_kinds.iter().map(|k| k.prefix.as_str()).collect();
    let before = prefixes.len();
    prefixes.sort_unstable();
    prefixes.dedup();
    assert_eq!(
        prefixes.len(),
        before,
        "two node kinds share a short id prefix"
    );
}
