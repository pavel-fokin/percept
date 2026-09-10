use super::*;
use std::fs;

fn tool() -> (tempfile::TempDir, ListFiles) {
    let (dir, workspace) = crate::workspace::temp_workspace();
    (dir, ListFiles::new(workspace))
}

#[test]
fn lists_files_and_directories_with_the_slash() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();

    let out = tool.run(r#"{}"#).unwrap();

    assert_eq!(out.content, "a.txt\nsub/");
}

#[test]
fn entries_are_sorted() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("b.txt"), "").unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();

    let out = tool.run(r#"{}"#).unwrap();

    assert_eq!(out.content, "a.txt\nb.txt");
}

#[test]
fn a_file_path_errors() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("a.txt"), "").unwrap();

    let err = tool.run(r#"{"path": "a.txt"}"#).err().unwrap().to_string();

    assert!(err.contains("not a directory"), "{err}");
}
