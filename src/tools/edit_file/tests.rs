use super::*;
use std::fs;

fn tool() -> (tempfile::TempDir, EditFile) {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Arc::new(Workspace::new(dir.path()).unwrap());
    (dir, EditFile::new(workspace))
}

fn write_and_read(dir: &tempfile::TempDir, workspace: &Workspace, name: &str, content: &str) {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    let resolved = workspace.resolve(name).unwrap();
    workspace.mark_read(&resolved);
}

#[test]
fn replaces_a_unique_string() {
    let (dir, tool) = tool();
    write_and_read(&dir, &tool.workspace, "a.txt", "hello world");

    let out = tool
        .run(r#"{"path": "a.txt", "old_string": "world", "new_string": "there"}"#)
        .unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "hello there"
    );
    assert_eq!(out.content, "edited a.txt: 1 replacement(s)");
}

#[test]
fn an_unread_file_is_refused() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("a.txt"), "hello world").unwrap();

    let err = tool
        .run(r#"{"path": "a.txt", "old_string": "world", "new_string": "there"}"#)
        .err()
        .unwrap()
        .to_string();

    assert!(err.contains("has not been read"), "{err}");
}

#[test]
fn a_string_occurring_twice_is_refused_without_replace_all() {
    let (dir, tool) = tool();
    write_and_read(&dir, &tool.workspace, "a.txt", "cat cat");

    let err = tool
        .run(r#"{"path": "a.txt", "old_string": "cat", "new_string": "dog"}"#)
        .err()
        .unwrap()
        .to_string();

    assert!(err.contains("occurs 2 times"), "{err}");
}

#[test]
fn replace_all_replaces_every_occurrence_and_reports_the_count() {
    let (dir, tool) = tool();
    write_and_read(&dir, &tool.workspace, "a.txt", "cat cat cat");

    let out = tool
        .run(r#"{"path": "a.txt", "old_string": "cat", "new_string": "dog", "replace_all": true}"#)
        .unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "dog dog dog"
    );
    assert_eq!(out.content, "edited a.txt: 3 replacement(s)");
}

#[test]
fn a_missing_old_string_is_refused() {
    let (dir, tool) = tool();
    write_and_read(&dir, &tool.workspace, "a.txt", "hello world");

    let err = tool
        .run(r#"{"path": "a.txt", "old_string": "goodbye", "new_string": "there"}"#)
        .err()
        .unwrap()
        .to_string();

    assert!(err.contains("was not found"), "{err}");
}

#[test]
fn identical_strings_are_refused() {
    let (dir, tool) = tool();
    write_and_read(&dir, &tool.workspace, "a.txt", "hello world");

    let err = tool
        .run(r#"{"path": "a.txt", "old_string": "world", "new_string": "world"}"#)
        .err()
        .unwrap()
        .to_string();

    assert!(err.contains("are the same"), "{err}");
}

#[test]
fn an_empty_old_string_is_refused_even_with_replace_all() {
    let (dir, tool) = tool();
    write_and_read(&dir, &tool.workspace, "a.txt", "abc");

    let err = tool
        .run(r#"{"path": "a.txt", "old_string": "", "new_string": "x", "replace_all": true}"#)
        .err()
        .unwrap();

    assert_eq!(err.to_string(), "old_string is empty");
    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "abc");
}

#[test]
fn a_file_that_is_not_utf8_is_refused_untouched() {
    let (dir, tool) = tool();
    let path = dir.path().join("latin1.txt");
    fs::write(&path, b"caf\xe9 foo").unwrap();
    let resolved = tool.workspace.resolve("latin1.txt").unwrap();
    tool.workspace.mark_read(&resolved);

    let err = tool
        .run(r#"{"path": "latin1.txt", "old_string": "foo", "new_string": "bar"}"#)
        .err()
        .unwrap();

    assert_eq!(err.to_string(), "latin1.txt is not UTF-8");
    assert_eq!(fs::read(&path).unwrap(), b"caf\xe9 foo");
}
