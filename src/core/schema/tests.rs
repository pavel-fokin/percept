use super::*;
use crate::core::testing::{debates, schemas, FakeSchemas};

fn node(kind: &str, properties: &[(&str, &[&str])]) -> NodeKind {
    NodeKind::new(
        kind,
        properties
            .iter()
            .map(|(name, values)| {
                (
                    name.to_string(),
                    values.iter().map(|value| value.to_string()).collect(),
                )
            })
            .collect(),
    )
    .unwrap()
}

fn edge(kind: &str, from: &[&str], to: &[&str]) -> EdgeKind {
    EdgeKind::new(
        kind,
        from.iter().map(|kind| kind.to_string()).collect(),
        to.iter().map(|kind| kind.to_string()).collect(),
    )
    .unwrap()
}

#[test]
fn node_kind_defaults_its_prefix_to_the_name_s_first_letter_lowercased() {
    assert_eq!(node("Verdict", &[]).prefix(), "v");
    assert_eq!(node("chore", &[]).prefix(), "c");
}

#[test]
fn node_kind_keeps_its_prefix() {
    assert_eq!(
        NodeKind::with_prefix("fact", "fx", Vec::new())
            .unwrap()
            .prefix(),
        "fx"
    );
}

#[test]
fn a_schema_needs_a_node_kind() {
    let err = Schema::new("empty", "p", Vec::new(), Vec::new()).unwrap_err();

    assert_eq!(err.to_string(), "declares no node kinds");
}

#[test]
fn a_kind_name_must_not_be_blank() {
    let node = NodeKind::new(" ", Vec::new()).unwrap_err();
    let edge = EdgeKind::new(" ", vec!["node".to_string()], vec!["node".to_string()]).unwrap_err();

    assert_eq!(node.to_string(), "a node kind must not be blank");
    assert_eq!(edge.to_string(), "a edge kind must not be blank");
}

#[test]
fn a_property_name_must_not_be_blank() {
    let err = NodeKind::new("term", vec![(" ".to_string(), Vec::new())]).unwrap_err();

    assert_eq!(
        err.to_string(),
        "node kind \"term\" declares a blank property"
    );
}

#[test]
fn node_kind_prefixes_must_be_unique() {
    let err = Schema::new(
        "glossary",
        "p",
        vec![node("term", &[]), node("taxonomy", &[])],
        Vec::new(),
    )
    .unwrap_err();

    assert_eq!(
        err.to_string(),
        "node kinds \"term\" and \"taxonomy\" both take the short id prefix \"t\""
    );
}

#[test]
fn a_node_kind_carries_at_most_one_closed_list() {
    let err = NodeKind::new(
        "term",
        vec![
            (
                "state".to_string(),
                vec!["open".to_string(), "done".to_string()],
            ),
            (
                "kind".to_string(),
                vec!["word".to_string(), "phrase".to_string()],
            ),
        ],
    )
    .unwrap_err();

    assert_eq!(
        err.to_string(),
        "node kind \"term\" declares a second closed list, \"kind\"; a kind carries at most one, alongside \"state\""
    );
}

#[test]
fn a_closed_list_needs_two_values() {
    let err = NodeKind::new(
        "term",
        vec![("state".to_string(), vec!["open".to_string()])],
    )
    .unwrap_err();

    assert_eq!(
        err.to_string(),
        "node kind \"term\" declares fewer than two values for \"state\""
    );
}

#[test]
fn a_closed_list_value_must_not_be_blank() {
    let err = NodeKind::new(
        "term",
        vec![(
            "state".to_string(),
            vec!["open".to_string(), " ".to_string()],
        )],
    )
    .unwrap_err();

    assert_eq!(
        err.to_string(),
        "node kind \"term\" declares a blank value for \"state\""
    );
}

#[test]
fn a_closed_list_value_must_be_unique() {
    let err = NodeKind::new(
        "term",
        vec![(
            "state".to_string(),
            vec!["open".to_string(), "open".to_string()],
        )],
    )
    .unwrap_err();

    assert_eq!(
        err.to_string(),
        "node kind \"term\" declares the value \"open\" twice for \"state\""
    );
}

#[test]
fn an_edge_end_needs_a_node_kind() {
    let from = EdgeKind::new("relates", Vec::new(), vec!["term".to_string()]).unwrap_err();
    let to = EdgeKind::new("relates", vec!["term".to_string()], Vec::new()).unwrap_err();

    assert_eq!(
        from.to_string(),
        "edge kind \"relates\"'s from names no node kind"
    );
    assert_eq!(
        to.to_string(),
        "edge kind \"relates\"'s to names no node kind"
    );
}

#[test]
fn an_edge_end_must_name_a_declared_node_kind() {
    let err = Schema::new(
        "glossary",
        "p",
        vec![node("term", &[])],
        vec![edge("relates", &["term"], &["acronym"])],
    )
    .unwrap_err();

    assert_eq!(
        err.to_string(),
        "edge kind \"relates\"'s to names \"acronym\", which is not a declared node kind"
    );
}

#[test]
fn an_unknown_map_with_no_schemas_says_maps_are_none() {
    let schemas = FakeSchemas::new(Vec::new());
    assert_eq!(
        schemas.find("decisions").err().unwrap().to_string(),
        "no map named \"decisions\"; maps are none"
    );
}

#[test]
fn a_schema_is_found_by_name() {
    let schemas = schemas();
    assert_eq!(schemas.find("debates").unwrap().name(), "debates");
    assert_eq!(
        schemas.find("glossary").err().unwrap().to_string(),
        "no map named \"glossary\"; maps are debates, chores"
    );
}

#[test]
fn a_topic_declares_no_properties() {
    assert!(debates()
        .node_kind("topic")
        .unwrap()
        .properties()
        .is_empty());
}

#[test]
fn an_undeclared_kind_is_absent() {
    assert!(debates().node_kind("glossary").is_none());
}
