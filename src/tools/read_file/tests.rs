use super::*;
use std::fs;

fn tool() -> (tempfile::TempDir, ReadFile) {
    let (dir, workspace) = crate::tools::workspace::temp_workspace();
    (dir, ReadFile::new(workspace))
}

#[test]
fn returns_the_whole_of_a_small_file() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("a.txt"), "one\ntwo\nthree").unwrap();

    let out = tool.run(r#"{"path": "a.txt"}"#).unwrap();

    assert_eq!(out.content, "one\ntwo\nthree");
}

#[test]
fn offset_and_limit_cut_a_window_and_the_trailer_names_the_next_offset() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\nfour\nfive").unwrap();

    let out = tool
        .run(r#"{"path": "a.txt", "offset": 2, "limit": 2}"#)
        .unwrap();

    assert_eq!(
        out.content,
        "two\nthree\n[2 more lines; call again with offset 4]"
    );
}

#[test]
fn a_binary_file_is_refused() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("a.bin"), [0u8, 1, 2, 3]).unwrap();

    let err = tool.run(r#"{"path": "a.bin"}"#).err().unwrap().to_string();

    assert!(err.contains("is binary"), "{err}");
}

#[test]
fn a_path_outside_the_root_is_refused() {
    let (_dir, tool) = tool();

    assert!(tool.run(r#"{"path": "../outside.txt"}"#).is_err());
}

#[test]
fn a_read_marks_the_path_read() {
    let (dir, tool) = tool();
    fs::write(dir.path().join("a.txt"), "hi").unwrap();
    let workspace = tool.workspace.clone();

    tool.run(r#"{"path": "a.txt"}"#).unwrap();

    let resolved = workspace.resolve("a.txt").unwrap();
    assert!(workspace.was_read(&resolved));
}
