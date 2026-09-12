use super::*;
use crate::core::testing::{decisions, tasks};

#[test]
fn the_purpose_line_follows_the_map_name() {
    let text = describe(&decisions());
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("decisions"));
    assert_eq!(lines.next(), Some(decisions().purpose.as_str()));
}

#[test]
fn a_kind_with_requires_and_states_lists_both() {
    let text = describe(&tasks());
    let line = text
        .lines()
        .find(|line| line.contains("requires"))
        .expect("a line with `requires`");
    assert!(line.contains("requires why"));
    assert!(line.contains("state open | done | dropped"));
}

#[test]
fn relations_name_each_edge_kinds_ends() {
    let text = describe(&decisions());
    assert!(text.contains("option --answers--> question"));
    assert!(text.contains("decision --resolves--> question"));
    assert!(text.contains("decision --supersedes--> decision"));
    assert!(text.contains("question --reopens--> decision"));
}

#[test]
fn the_add_example_only_edges_to_kinds_already_listed() {
    let text = describe(&decisions());
    // `question` comes first, so `reopens decision` - a decision not
    // yet listed - is skipped; `decision` comes last, so `supersedes
    // decision` - itself, not yet listed - is skipped too.
    assert!(!text.contains("reopens decision"));
    assert!(!text.contains("supersedes decision"));
    // What is listed by the time each kind is reached does appear.
    assert!(text.contains("answers question"));
    assert!(text.contains("resolves question"));
    // One edge per neighbour: evidence supports the option it is shown
    // with, never also contradicting it.
    assert!(text.contains("supports option"));
    assert!(!text.contains("contradicts option"));
}

#[test]
fn the_add_example_sets_the_first_state_on_a_kind_that_declares_states() {
    let text = describe(&tasks());
    assert!(text.contains("  task \"...\"\n    why \"...\"\n    state \"open\"\n"), "{text}");
}

#[test]
fn the_change_example_is_present_for_a_schema_with_states() {
    let text = describe(&tasks());
    assert!(text.contains("example: change"));
    assert!(text.contains("t1"));
    assert!(text.contains("state \"done\""));
    assert!(text.contains("why \"what happened\""));
}

#[test]
fn the_change_example_is_absent_for_a_schema_with_no_states() {
    let text = describe(&decisions());
    assert!(!text.contains("example: change"));
}
