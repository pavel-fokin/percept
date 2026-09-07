use super::*;

fn tool() -> (tempfile::TempDir, Bash) {
    let (dir, workspace) = crate::tools::workspace::temp_workspace();
    (dir, Bash::new(workspace))
}

#[test]
fn stdout_comes_back_after_an_exit_0_line() {
    let (_dir, tool) = tool();

    let out = tool.run(r#"{"command": "echo hi"}"#).unwrap();

    assert_eq!(out.content, "exit 0\nhi\n");
}

#[test]
fn a_non_zero_exit_is_reported_not_errored() {
    let (_dir, tool) = tool();

    let out = tool.run(r#"{"command": "exit 3"}"#).unwrap();

    assert_eq!(out.content, "exit 3");
}

#[test]
fn stderr_appears_under_its_marker_only_when_present() {
    let (_dir, tool) = tool();

    let out = tool.run(r#"{"command": "echo oops 1>&2"}"#).unwrap();
    assert_eq!(out.content, "exit 0\n--- stderr ---\noops\n");

    let out = tool.run(r#"{"command": "echo fine"}"#).unwrap();
    assert!(!out.content.contains("stderr"), "{}", out.content);
}

#[test]
fn the_command_runs_at_the_workspace_root() {
    let (dir, tool) = tool();

    let out = tool.run(r#"{"command": "pwd"}"#).unwrap();

    assert_eq!(
        out.content,
        format!("exit 0\n{}\n", dir.path().canonicalize().unwrap().display())
    );
}

#[test]
fn a_sleep_past_the_timeout_errors_within_a_few_seconds() {
    let (_dir, tool) = tool();
    let start = std::time::Instant::now();

    let err = tool
        .run(r#"{"command": "sleep 5", "timeout_secs": 1}"#)
        .err()
        .unwrap()
        .to_string();

    assert!(err.contains("timed out after 1s"), "{err}");
    assert!(
        start.elapsed() < Duration::from_secs(4),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn a_background_process_holding_the_pipe_is_cut_off_at_the_timeout_not_waited_for() {
    let (_dir, tool) = tool();
    let start = std::time::Instant::now();

    let out = tool
        .run(r#"{"command": "sleep 5 & echo started", "timeout_secs": 1}"#)
        .unwrap();

    assert_eq!(out.content, "exit 0\nstarted\n");
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn a_description_field_is_accepted_and_ignored() {
    let (_dir, tool) = tool();

    let out = tool
        .run(r#"{"command": "echo hi", "description": "say hi"}"#)
        .unwrap();

    assert_eq!(out.content, "exit 0\nhi\n");
}

#[test]
fn the_command_runs_under_bash_so_a_bashism_works() {
    let (_dir, tool) = tool();

    let out = tool
        .run(r#"{"command": "[[ 1 == 1 ]] && echo ${BASH_VERSION:+bash}"}"#)
        .unwrap();

    assert_eq!(out.content, "exit 0\nbash\n");
}

#[test]
fn output_past_30000_characters_is_truncated_with_the_trailer() {
    let (_dir, tool) = tool();

    let out = tool.run(r#"{"command": "yes x | head -c 40000"}"#).unwrap();

    assert!(out
        .content
        .contains("[output truncated at 30000 characters]"));
    let limit_line_len = "[output truncated at 30000 characters]".len();
    assert_eq!(out.content.len(), OUTPUT_LIMIT + 1 + limit_line_len);
}
