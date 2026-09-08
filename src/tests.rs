use std::fs;
use std::sync::Arc;

use tempfile::tempdir;

use super::{
    discover_root, resolve_toolset, LogMaps, ReadMap, RoutedMaps, Toolset, CODE_SOURCE_NAME,
};
use crate::core::testing::{scope, FakeLog};
use crate::core::MapReader;
use crate::harness::Tool;

#[test]
fn tui_in_a_git_checkout_defaults_to_code_tools() {
    assert!(matches!(
        resolve_toolset(CODE_SOURCE_NAME, None, true),
        Ok(Toolset::Code)
    ));
}

#[test]
fn tui_without_a_git_checkout_defaults_to_maps_tools() {
    assert!(matches!(
        resolve_toolset(CODE_SOURCE_NAME, None, false),
        Ok(Toolset::Maps)
    ));
}

#[test]
fn headless_source_defaults_to_maps_tools() {
    assert!(matches!(
        resolve_toolset("percept-cli", None, true),
        Ok(Toolset::Maps)
    ));
}

#[test]
fn explicit_toolset_overrides_client_default() {
    assert!(matches!(
        resolve_toolset(CODE_SOURCE_NAME, Some("maps"), true),
        Ok(Toolset::Maps)
    ));
    assert!(matches!(
        resolve_toolset("percept-cli", Some("code"), false),
        Ok(Toolset::Code)
    ));
}

#[test]
fn unknown_toolset_is_rejected() {
    assert!(resolve_toolset(CODE_SOURCE_NAME, Some("unknown"), true).is_err());
}

#[test]
fn routed_maps_walks_the_working_tree_for_the_code_map() {
    let tree = tempdir().unwrap();
    fs::write(
        tree.path().join("lib.rs"),
        "pub fn answer() -> u32 { 42 }\n",
    )
    .unwrap();
    let maps = RoutedMaps {
        folded: LogMaps::new(Arc::new(FakeLog::default()), scope()),
        root: tree.path().to_path_buf(),
    };

    let code = maps.read("code").unwrap();

    assert!(code
        .nodes()
        .iter()
        .any(|node| node.kind == "file" && node.name == "lib.rs"));
    assert!(code
        .nodes()
        .iter()
        .any(|node| node.kind == "function" && node.name.ends_with("::answer")));
}

#[test]
fn routed_maps_folds_every_other_map_from_the_log() {
    let maps = RoutedMaps {
        folded: LogMaps::new(Arc::new(FakeLog::default()), scope()),
        root: tempdir().unwrap().path().to_path_buf(),
    };

    assert_eq!(maps.read("decisions").unwrap().nodes().len(), 0);
    assert!(maps.read("plans").is_err());
}

#[test]
fn read_map_refuses_since_on_the_code_map() {
    let tree = tempdir().unwrap();
    fs::write(tree.path().join("lib.rs"), "pub fn f() {}\n").unwrap();
    let tool = ReadMap::new(Arc::new(RoutedMaps {
        folded: LogMaps::new(Arc::new(FakeLog::default()), scope()),
        root: tree.path().to_path_buf(),
    }));

    let Err(err) = tool.run(r#"{"map":"code","since":"1d"}"#) else {
        panic!("expected an error")
    };

    assert!(err.to_string().contains("no meaning for"), "{err}");
}

#[test]
fn finds_a_git_checkout_from_a_nested_directory() {
    let root = tempdir().unwrap();
    fs::create_dir(root.path().join(".git")).unwrap();
    let nested = root.path().join("src/cli");
    fs::create_dir_all(&nested).unwrap();

    assert_eq!(
        discover_root(&nested, Some(root.path().parent().unwrap())),
        Some(root.path().to_path_buf())
    );
}

#[test]
fn finds_a_percept_directory_when_there_is_no_git() {
    let root = tempdir().unwrap();
    fs::create_dir(root.path().join(".percept")).unwrap();

    assert_eq!(
        discover_root(root.path(), Some(root.path().parent().unwrap())),
        Some(root.path().to_path_buf())
    );
}

#[test]
fn stops_at_home_before_matching_a_marker_above_it() {
    let home = tempdir().unwrap();
    fs::create_dir(home.path().join(".git")).unwrap();
    let project = home.path().join("code/app");
    fs::create_dir_all(&project).unwrap();

    assert_eq!(discover_root(&project, Some(home.path())), None);
}

#[test]
fn ignores_a_percept_directory_at_home_itself() {
    let home = tempdir().unwrap();
    fs::create_dir(home.path().join(".percept")).unwrap();

    assert_eq!(discover_root(home.path(), Some(home.path())), None);
}
