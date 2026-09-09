use std::fs;

use tempfile::tempdir;

use super::*;

#[test]
fn spec_names_the_tool_and_carries_valid_schema_json() {
    let tree = tempdir().unwrap();
    let spec = ReadCode::new(tree.path().to_path_buf()).spec();

    assert_eq!(spec.name, "read_code");
    let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
    assert_eq!(schema["type"], "object");
}

#[test]
fn a_read_returns_the_walk_s_files_and_symbols() {
    let tree = tempdir().unwrap();
    fs::write(
        tree.path().join("lib.rs"),
        "pub fn answer() -> u32 { 42 }\n",
    )
    .unwrap();

    let out = ReadCode::new(tree.path().to_path_buf()).run("{}").unwrap();

    assert!(out.content.contains("lib.rs"));
    assert!(out.content.contains("::answer"));
}

#[test]
fn a_node_line_carries_no_actor_or_time() {
    let tree = tempdir().unwrap();
    fs::write(
        tree.path().join("lib.rs"),
        "pub fn answer() -> u32 { 42 }\n",
    )
    .unwrap();

    let out = ReadCode::new(tree.path().to_path_buf()).run("{}").unwrap();

    let line = out
        .content
        .lines()
        .find(|line| line.contains("\"node\""))
        .expect("a node line");
    let json: serde_json::Value = serde_json::from_str(line).unwrap();
    assert!(json.get("actor").is_none(), "{json}");
    assert!(json.get("added_at").is_none(), "{json}");
}

#[test]
fn every_call_walks_the_tree_fresh() {
    let tree = tempdir().unwrap();
    let tool = ReadCode::new(tree.path().to_path_buf());
    let before = tool.run("{}").unwrap();
    assert!(!before.content.contains("lib.rs"));

    fs::write(
        tree.path().join("lib.rs"),
        "pub fn answer() -> u32 { 42 }\n",
    )
    .unwrap();
    let after = tool.run("{}").unwrap();

    assert!(after.content.contains("lib.rs"));
}
