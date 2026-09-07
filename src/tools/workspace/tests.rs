use super::*;
use std::fs;

#[cfg(unix)]
use std::os::unix::fs::symlink;

fn workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::new(dir.path()).unwrap();
    (dir, workspace)
}

#[test]
fn a_relative_path_resolves_under_the_root() {
    let (dir, workspace) = workspace();
    fs::write(dir.path().join("a.txt"), "hi").unwrap();

    let resolved = workspace.resolve("a.txt").unwrap();

    assert_eq!(resolved, dir.path().canonicalize().unwrap().join("a.txt"));
}

#[test]
fn dot_dot_past_the_root_is_refused() {
    let (_dir, workspace) = workspace();

    assert!(workspace.resolve("../outside.txt").is_err());
}

#[test]
fn an_absolute_path_outside_the_root_is_refused() {
    let (_dir, workspace) = workspace();

    assert!(workspace.resolve("/etc/hosts").is_err());
}

#[test]
#[cfg(unix)]
fn a_symlink_inside_the_tree_pointing_outside_is_refused() {
    let (dir, workspace) = workspace();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "shh").unwrap();
    symlink(outside.path(), dir.path().join("link")).unwrap();

    assert!(workspace.resolve("link/secret.txt").is_err());
}

#[test]
#[cfg(unix)]
fn a_dangling_symlink_pointing_outside_is_refused_not_treated_as_a_new_file() {
    let (dir, workspace) = workspace();
    let outside = tempfile::tempdir().unwrap();
    symlink(
        outside.path().join("absent.txt"),
        dir.path().join("link.txt"),
    )
    .unwrap();

    assert!(workspace.resolve("link.txt").is_err());
}

#[test]
fn walk_enters_dot_directories_but_not_git_itself() {
    let (dir, workspace) = workspace();
    fs::create_dir_all(dir.path().join(".percept")).unwrap();
    fs::write(dir.path().join(".percept/index.md"), "").unwrap();
    fs::create_dir_all(dir.path().join(".git/objects")).unwrap();
    fs::write(dir.path().join(".git/HEAD"), "").unwrap();

    let seen: Vec<String> = workspace
        .walk(workspace.root())
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .map(|entry| workspace.relative(entry.path()))
        .collect();

    assert_eq!(seen, vec![".percept/index.md"]);
}

#[test]
fn a_path_that_does_not_exist_yet_under_an_existing_directory_resolves() {
    let (dir, workspace) = workspace();

    let resolved = workspace.resolve("new.txt").unwrap();

    assert_eq!(resolved, dir.path().canonicalize().unwrap().join("new.txt"));
}

#[test]
fn a_path_under_directories_that_do_not_exist_yet_resolves_without_a_trailing_separator() {
    let (dir, workspace) = workspace();

    let resolved = workspace.resolve("a/b/new.txt").unwrap();

    // Compared as strings: `Path` equality ignores a trailing separator
    // that `fs::write` does not.
    let expected = dir.path().canonicalize().unwrap().join("a/b/new.txt");
    assert_eq!(resolved.as_os_str(), expected.as_os_str());
}

#[test]
fn relative_strips_the_root() {
    let (dir, workspace) = workspace();
    let path = dir.path().canonicalize().unwrap().join("src/main.rs");

    assert_eq!(workspace.relative(&path), "src/main.rs");
}
