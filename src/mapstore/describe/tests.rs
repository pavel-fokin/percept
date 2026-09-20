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
fn the_add_example_edges_to_kinds_written_above() {
    let text = describe(&debates());
    // Kinds are written finest first, so an edge from a coarser kind
    // finds its target above it: `backs` from claim, `about` and
    // `settles` from topic. `doubts` and `replaces` point at a kind
    // not yet written when their own is, so neither appears.
    assert!(text.contains("backs fact"), "{text}");
    assert!(text.contains("about claim"), "{text}");
    assert!(text.contains("settles verdict"), "{text}");
    assert!(!text.contains("doubts topic"), "{text}");
    assert!(!text.contains("replaces verdict"), "{text}");
}

#[test]
fn the_add_example_points_one_edge_at_a_kind_two_others_may_reach() {
    let schema = crate::core::Schema {
        name: "court".to_string(),
        purpose: "test fixture".to_string(),
        node_kinds: vec![
            crate::core::NodeKind::new("hearing"),
            crate::core::NodeKind::new("motion"),
            crate::core::NodeKind::new("ruling"),
        ],
        edge_kinds: vec![
            crate::core::EdgeKind::new("opens", &["hearing"], &["ruling"]),
            crate::core::EdgeKind::new("seeks", &["motion"], &["ruling"]),
        ],
        rules: crate::core::Rules::default(),
    };

    let text = describe(&schema);

    // A second edge into `ruling` would be a second parent, which
    // `apply` refuses, so the example shows only the first.
    assert!(text.contains("seeks ruling"), "{text}");
    assert!(!text.contains("opens ruling"), "{text}");
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
        text.contains("  topic \"...\"\n    about claim\n    settles verdict\n    cites src/path.rs:10-20\n"),
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
