use std::fs;

use super::walk;
use crate::workspace::temp_workspace;

#[test]
fn walk_enters_dot_directories_but_not_git_itself() {
    let (dir, workspace) = temp_workspace();
    fs::create_dir_all(dir.path().join(".percept")).unwrap();
    fs::write(dir.path().join(".percept/index.md"), "").unwrap();
    fs::create_dir_all(dir.path().join(".git/objects")).unwrap();
    fs::write(dir.path().join(".git/HEAD"), "").unwrap();

    let seen: Vec<String> = walk(workspace.root())
        .map(|entry| workspace.relative(entry.path()))
        .collect();

    assert_eq!(seen, vec![".percept/index.md"]);
}
