use std::fs;

use tempfile::tempdir;

use super::discover_root;

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
fn stops_at_home_and_takes_it_as_the_root() {
    let home = tempdir().unwrap();
    fs::create_dir(home.path().join(".git")).unwrap();
    let project = home.path().join("code/app");
    fs::create_dir_all(&project).unwrap();

    assert_eq!(discover_root(&project, Some(home.path())), Some(home.path().to_path_buf()));
}

#[test]
fn home_itself_is_the_root() {
    let home = tempdir().unwrap();

    assert_eq!(discover_root(home.path(), Some(home.path())), Some(home.path().to_path_buf()));
}

#[test]
fn outside_home_with_no_marker_there_is_no_root() {
    let dir = tempdir().unwrap();
    let home = tempdir().unwrap();

    assert_eq!(discover_root(dir.path(), Some(home.path())), None);
}
