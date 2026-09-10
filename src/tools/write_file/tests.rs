use super::*;
use std::fs;

fn tool() -> (tempfile::TempDir, WriteFile) {
    let (dir, workspace) = crate::workspace::temp_workspace();
    (dir, WriteFile::new(workspace))
}

#[test]
fn creates_a_file_with_missing_parent_directories() {
    let (dir, tool) = tool();

    let out = tool
        .run(r#"{"path": "a/b/c.txt", "content": "hi"}"#)
        .unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("a/b/c.txt")).unwrap(),
        "hi"
    );
    assert_eq!(out.content, "wrote 2 bytes to a/b/c.txt");
}

#[test]
fn overwrites_an_existing_file() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("a.txt"), "old").unwrap();

    tool.run(r#"{"path": "a.txt", "content": "new"}"#).unwrap();

    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "new");
}

#[test]
fn a_path_outside_the_root_is_refused_and_nothing_is_written() {
    let (dir, tool) = tool();

    let err = tool
        .run(r#"{"path": "../outside.txt", "content": "hi"}"#)
        .err();

    assert!(err.is_some());
    assert!(!dir.path().parent().unwrap().join("outside.txt").exists());
}

#[test]
fn the_written_path_counts_as_read() {
    let (dir, tool) = tool();

    tool.run(r#"{"path": "a.txt", "content": "hi"}"#).unwrap();

    let workspace = tool.workspace.clone();
    let resolved = workspace
        .resolve(dir.path().join("a.txt").to_str().unwrap())
        .unwrap();
    assert!(workspace.was_read(&resolved));
}
