use super::*;
use std::fs;

/// A repository with one committed file and one ignored path.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-q"]);
    fs::write(root.join("kept.txt"), "original\n").unwrap();
    fs::write(root.join(".gitignore"), "ignored.log\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "base"]);
    dir
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    // Only the trailing newline goes: porcelain status leads with a
    // space when a change is unstaged, and that space is the point.
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

#[test]
fn restore_puts_a_changed_file_back_as_the_snapshot_saw_it() {
    let dir = repo();
    let snapshot = GitSnapshot::open(dir.path()).unwrap();
    let prompt = EventId::new();
    fs::write(dir.path().join("kept.txt"), "before the turn\n").unwrap();

    snapshot.take(prompt).unwrap();
    fs::write(dir.path().join("kept.txt"), "changed by the turn\n").unwrap();
    snapshot.restore(prompt).unwrap();

    let text = fs::read_to_string(dir.path().join("kept.txt")).unwrap();
    assert_eq!(text, "before the turn\n");
}

#[test]
fn restore_removes_a_file_the_turn_created_and_keeps_an_ignored_one() {
    let dir = repo();
    let snapshot = GitSnapshot::open(dir.path()).unwrap();
    let prompt = EventId::new();
    fs::write(dir.path().join("ignored.log"), "log\n").unwrap();

    snapshot.take(prompt).unwrap();
    fs::write(dir.path().join("new.txt"), "made by the turn\n").unwrap();
    snapshot.restore(prompt).unwrap();

    assert!(!dir.path().join("new.txt").exists());
    assert!(dir.path().join("ignored.log").exists());
}

#[test]
fn restore_brings_back_an_untracked_file_the_turn_deleted() {
    let dir = repo();
    let snapshot = GitSnapshot::open(dir.path()).unwrap();
    let prompt = EventId::new();
    fs::write(dir.path().join("draft.txt"), "untracked before\n").unwrap();

    snapshot.take(prompt).unwrap();
    fs::remove_file(dir.path().join("draft.txt")).unwrap();
    snapshot.restore(prompt).unwrap();

    let text = fs::read_to_string(dir.path().join("draft.txt")).unwrap();
    assert_eq!(text, "untracked before\n");
    // Restored as it was: on disk, not staged.
    assert_eq!(
        git_stdout(dir.path(), &["status", "--porcelain", "draft.txt"]),
        "?? draft.txt"
    );
}

#[test]
fn take_leaves_the_branch_and_index_untouched_and_keeps_the_snapshot_under_its_own_ref() {
    let dir = repo();
    let snapshot = GitSnapshot::open(dir.path()).unwrap();
    let prompt = EventId::new();
    let head_before = git_stdout(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("kept.txt"), "dirty\n").unwrap();

    snapshot.take(prompt).unwrap();

    assert_eq!(git_stdout(dir.path(), &["rev-parse", "HEAD"]), head_before);
    assert_eq!(
        git_stdout(dir.path(), &["status", "--porcelain"]),
        " M kept.txt"
    );
    let snapshot_ref = format!("refs/percept/snapshots/{}", prompt.as_uuid());
    let parent = git_stdout(dir.path(), &["rev-parse", &format!("{snapshot_ref}^")]);
    assert_eq!(parent, head_before);
}

#[test]
fn opening_a_directory_that_is_not_a_repository_is_an_error() {
    let dir = tempfile::tempdir().unwrap();

    assert!(GitSnapshot::open(dir.path()).is_err());
}

fn snapshot_refs(root: &Path) -> String {
    git_stdout(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/percept/snapshots",
        ],
    )
}

#[test]
fn take_keeps_only_the_newest_snapshot_ref() {
    let dir = repo();
    let snapshot = GitSnapshot::open(dir.path()).unwrap();
    let first = EventId::new();
    let second = EventId::new();

    snapshot.take(first).unwrap();
    snapshot.take(second).unwrap();

    assert_eq!(
        snapshot_refs(dir.path()),
        format!("refs/percept/snapshots/{}", second.as_uuid())
    );
    assert!(snapshot.restore(first).is_err());
}

#[test]
fn open_deletes_refs_an_earlier_session_left_behind() {
    let dir = repo();
    let earlier = GitSnapshot::open(dir.path()).unwrap();
    earlier.take(EventId::new()).unwrap();
    drop(earlier);
    assert!(!snapshot_refs(dir.path()).is_empty());

    let _later = GitSnapshot::open(dir.path()).unwrap();

    assert!(snapshot_refs(dir.path()).is_empty());
}

#[test]
fn restoring_a_prompt_never_snapshotted_is_an_error() {
    let dir = repo();
    let snapshot = GitSnapshot::open(dir.path()).unwrap();

    let err = snapshot.restore(EventId::new()).unwrap_err();

    assert!(err.to_string().starts_with("no snapshot for prompt"));
}

#[test]
fn a_repository_with_no_commit_yet_can_still_be_snapshotted_and_restored() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .unwrap()
        .success());
    let snapshot = GitSnapshot::open(dir.path()).unwrap();
    let prompt = EventId::new();
    fs::write(dir.path().join("first.txt"), "one\n").unwrap();

    snapshot.take(prompt).unwrap();
    fs::write(dir.path().join("first.txt"), "two\n").unwrap();
    snapshot.restore(prompt).unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("first.txt")).unwrap(),
        "one\n"
    );
}
