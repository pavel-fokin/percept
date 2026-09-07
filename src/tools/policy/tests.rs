use super::*;

#[test]
fn a_write_an_edit_and_a_command_are_put_to_the_user() {
    for tool in ["write_file", "edit_file", "bash"] {
        assert_eq!(AskBeforeWrites.check(tool, "{}"), Verdict::Ask, "{tool}");
    }
}

#[test]
fn a_read_runs_unasked() {
    for tool in ["read_file", "grep_files", "search_events", "read_map"] {
        assert_eq!(AskBeforeWrites.check(tool, "{}"), Verdict::Allow, "{tool}");
    }
}
