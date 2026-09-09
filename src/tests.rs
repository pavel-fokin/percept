use std::fs;

use tempfile::tempdir;

use super::{code_tools, discover_root, resolve_toolset, Toolset, CODE_SOURCE_NAME};
use crate::harness::Tool;
use crate::tools::ReadCode;

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
fn code_tools_offers_read_code() {
    let tree = tempdir().unwrap();

    let tools = code_tools(tree.path()).unwrap();

    assert!(tools.iter().any(|tool| tool.spec().name == "read_code"));
}

#[test]
fn read_code_walks_the_working_tree_it_is_given() {
    let tree = tempdir().unwrap();
    fs::write(
        tree.path().join("lib.rs"),
        "pub fn answer() -> u32 { 42 }\n",
    )
    .unwrap();
    let tool = ReadCode::new(tree.path().to_path_buf());

    let out = tool.run("{}").unwrap();

    assert!(out.content.contains("lib.rs"));
    assert!(out.content.contains("::answer"));
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
