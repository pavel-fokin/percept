use std::fs;

use tempfile::tempdir;

use super::{discover_root, resolve_toolset, Toolset, TUI_SOURCE_NAME};

#[test]
fn tui_defaults_to_code_tools() {
    assert!(matches!(
        resolve_toolset(TUI_SOURCE_NAME, None),
        Ok(Toolset::Code)
    ));
}

#[test]
fn headless_source_defaults_to_maps_tools() {
    assert!(matches!(
        resolve_toolset("percept-cli", None),
        Ok(Toolset::Maps)
    ));
}

#[test]
fn explicit_toolset_overrides_client_default() {
    assert!(matches!(
        resolve_toolset(TUI_SOURCE_NAME, Some("maps")),
        Ok(Toolset::Maps)
    ));
    assert!(matches!(
        resolve_toolset("percept-cli", Some("code")),
        Ok(Toolset::Code)
    ));
}

#[test]
fn unknown_toolset_is_rejected() {
    assert!(resolve_toolset(TUI_SOURCE_NAME, Some("unknown")).is_err());
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
