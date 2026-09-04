use super::*;
use std::fs;

/// A scratch directory holding fixture Rust sources, torn down when the
/// test ends - never the repository itself.
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// `ignore` only reads `.gitignore` files inside an actual git
    /// (or jj) repository, so a fixture that wants one respected needs
    /// a `.git` directory too, empty though it is here.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        Self { dir }
    }

    /// Writes `content` at `path`, relative to the fixture's root,
    /// creating any directories it needs.
    fn write(&self, path: &str, content: &str) -> &Self {
        let full = self.dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
        self
    }

    fn build(&self) -> Map {
        build(self.dir.path()).unwrap()
    }
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

    let map = fixture.build();

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

    let map = fixture.build();

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

    let map = fixture.build();

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

    let map = fixture.build();

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

    let map = fixture.build();

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

    let map = fixture.build();

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

    let map = fixture.build();

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

    let map = fixture.build();

    assert!(map.find("file", "src/generated.rs").is_none());
    assert!(map.edges().is_empty());
}

#[test]
fn a_functions_symbol_carries_public_and_line_and_is_contained_by_its_file() {
    let fixture = Fixture::new();
    fixture.write("src/main.rs", "\npub fn greet() {}\n");

    let map = fixture.build();

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

    let map = fixture.build();

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

    let map = fixture.build();

    assert!(map.find("package", "app").is_none());
    assert!(has_edge(
        &map,
        ("file", "src/main.rs"),
        "imports",
        ("file", "src/app/mod.rs"),
    ));
}
