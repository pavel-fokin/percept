use super::*;
use crate::core::testing::{chores, debates};

#[test]
fn the_purpose_line_follows_the_map_name() {
    let text = describe(&debates());
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("debates"));
    assert_eq!(lines.next(), Some(debates().purpose.as_str()));
}

#[test]
fn a_kind_with_requires_and_states_lists_both() {
    let text = describe(&chores());
    let line = text
        .lines()
        .find(|line| line.contains("requires"))
        .expect("a line with `requires`");
    assert!(line.contains("requires why"));
    assert!(line.contains("state open | done | dropped"));
}

#[test]
fn relations_name_each_edge_kinds_ends() {
    let text = describe(&debates());
    assert!(text.contains("claim --about--> topic"));
    assert!(text.contains("verdict --settles--> topic"));
    assert!(text.contains("verdict --replaces--> verdict"));
    assert!(text.contains("topic --doubts--> verdict"));
}

#[test]
fn the_add_example_only_edges_to_kinds_already_listed() {
    let text = describe(&debates());
    // `topic` comes first, so `doubts verdict` - a verdict not
    // yet listed - is skipped; `verdict` comes last, so `replaces
    // verdict` - itself, not yet listed - is skipped too.
    assert!(!text.contains("doubts verdict"));
    assert!(!text.contains("replaces verdict"));
    // What is listed by the time each kind is reached does appear.
    assert!(text.contains("about topic"));
    assert!(text.contains("settles topic"));
    assert!(text.contains("backs claim"));
}

#[test]
fn the_add_example_sets_the_first_state_on_a_kind_that_declares_states() {
    let text = describe(&chores());
    assert!(text.contains("  chore \"...\"\n    why \"...\"\n    state \"open\"\n"), "{text}");
}

#[test]
fn the_grammar_is_indented_under_the_record_command() {
    let text = describe(&debates());
    assert!(text.contains("\n  A line at the margin"), "{text}");
}

#[test]
fn the_add_example_cites_a_file_under_its_last_node() {
    let text = describe(&debates());
    assert!(
        text.contains("  verdict \"...\"\n    settles topic\n    cites src/path.rs:10-20\n"),
        "{text}"
    );
}

#[test]
fn the_change_example_is_present_for_a_schema_with_states() {
    let text = describe(&chores());
    assert!(text.contains("example: change"));
    assert!(text.contains("c1"));
    assert!(text.contains("state \"done\""));
    assert!(text.contains("why \"what happened\""));
}

#[test]
fn the_change_example_is_absent_for_a_schema_with_no_states() {
    let text = describe(&debates());
    assert!(!text.contains("example: change"));
}
