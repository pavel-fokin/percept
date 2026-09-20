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
fn the_record_grammar_names_the_commands_that_remove() {
    let text = describe(&debates());

    assert!(text.contains("remove-node"), "{text:?}");
    assert!(text.contains("remove-edge"), "{text:?}");
}

#[test]
fn a_kind_with_free_properties_and_a_closed_list_lists_both() {
    let text = describe(&chores());
    let line = text
        .lines()
        .find(|line| line.contains("why"))
        .expect("a line with `why`");
    assert!(line.contains("why"));
    assert!(line.contains("state open | done | dropped"));
}

#[test]
fn a_relation_line_names_its_ends() {
    let schema = crate::core::Schema {
        name: "court".to_string(),
        purpose: "test fixture".to_string(),
        node_kinds: vec![crate::core::NodeKind::new("topic"), crate::core::NodeKind::new("verdict")],
        edge_kinds: vec![crate::core::EdgeKind::new("settles", &["verdict"], &["topic"])],
        rules: crate::core::Rules::default(),
    };

    let text = describe(&schema);

    assert!(text.contains("verdict --settles--> topic\n"), "{text}");
}

#[test]
fn relations_name_each_edge_kinds_ends() {
    let text = describe(&debates());
    assert!(text.contains("topic --about--> claim"));
    assert!(text.contains("topic --settles--> verdict"));
    assert!(text.contains("verdict --replaces--> verdict"));
    assert!(text.contains("verdict --doubts--> topic"));
}

#[test]
fn the_add_example_only_edges_to_kinds_already_listed() {
    let text = describe(&debates());
    // `about`, `settles`, and `backs` each run from a coarser kind to
    // a finer one, so their target kind's node has not yet appeared
    // in the example when the source kind is written; `replaces`
    // points at its own kind, not yet listed either. Only `doubts`,
    // which points back from `verdict`, the last kind, to `topic`,
    // the first, finds its target already listed.
    assert!(!text.contains("about claim"));
    assert!(!text.contains("settles verdict"));
    assert!(!text.contains("backs fact"));
    assert!(!text.contains("replaces verdict"));
    assert!(text.contains("doubts topic"));
}

#[test]
fn the_add_example_sets_the_first_value_on_a_kind_with_a_closed_list() {
    let text = describe(&chores());
    assert!(
        text.contains("  chore \"...\"\n    why \"...\"\n    outcome \"...\"\n    state \"open\"\n"),
        "{text}"
    );
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
        text.contains("  verdict \"...\"\n    why \"...\"\n    doubts topic\n    cites src/path.rs:10-20\n"),
        "{text}"
    );
}

#[test]
fn the_change_example_is_present_for_a_schema_with_states() {
    let text = describe(&chores());
    assert!(text.contains("example: change"));
    assert!(text.contains("c1"));
    assert!(text.contains("state \"done\""));
}

#[test]
fn the_change_example_is_absent_for_a_schema_with_no_states() {
    let text = describe(&debates());
    assert!(!text.contains("example: change"));
}

#[test]
fn a_schema_with_rules_prints_them_in_a_rules_section() {
    let mut schema = debates();
    schema.rules = crate::core::Rules::new(std::collections::BTreeMap::from([(
        "message.received".to_string(),
        vec!["first rule".to_string(), "second rule".to_string()],
    )]));

    let text = describe(&schema);

    assert!(
        text.contains(
            "\nrules\n  message.received   first rule\n                     second rule\n"
        ),
        "{text}"
    );
}

#[test]
fn a_schema_with_rules_at_two_moments_lists_both() {
    let mut schema = debates();
    schema.rules = crate::core::Rules::new(std::collections::BTreeMap::from([
        ("session.started".to_string(), vec!["s".to_string()]),
        ("message.received".to_string(), vec!["m".to_string()]),
    ]));

    let text = describe(&schema);

    assert!(text.contains("session.started"), "{text}");
    assert!(text.contains("message.received"), "{text}");
}

#[test]
fn a_schema_with_no_rules_prints_no_rules_section() {
    let text = describe(&debates());
    assert!(!text.contains("\nrules\n"), "{text}");
}
