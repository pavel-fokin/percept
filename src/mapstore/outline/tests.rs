use std::collections::BTreeMap;

use super::*;
use crate::core::testing::{chores, court, debates, human, link, map_id};
use crate::core::{Actor, Map, Mutation};

/// Adds a node, with whatever properties its kind requires: a `why`
/// when it requires one, and a `state` when it declares any - `claim`
/// and `chore` both require `why`, and `chore` declares states.
fn add(map: &mut Map, kind: &str, name: &str) {
    let mut properties = BTreeMap::new();
    if let Some(node_kind) = map.schema().node_kind(kind) {
        if node_kind.requires.iter().any(|p| p == "why") {
            properties.insert("why".to_string(), "because".to_string());
        }
        if let Some(state) = node_kind.states.first() {
            properties.insert("state".to_string(), state.clone());
        }
    }
    map.apply(
        Mutation::AddNode {
            kind: kind.to_string(),
            name: name.to_string(),
            properties,
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
}


#[test]
fn a_node_of_any_kind_nobody_claims_heads_a_section() {
    // `fact` heads no section in any schema that declares it a leaf -
    // but nothing points at this one, so the outline shows it rather
    // than dropping it. An unattached node is debt a reader should
    // see.
    let mut map = Map::empty(map_id("debates"), debates());
    add(&mut map, "fact", "benchmarks");

    let names: Vec<&str> = roots(&map).into_iter().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["benchmarks"]);
}

#[test]
fn a_node_its_neighbour_claims_leaves_the_roots() {
    // `backs` runs from `fact` to `claim`, so the claim claims the
    // fact and only the claim heads a section.
    let mut map = Map::empty(map_id("debates"), debates());
    add(&mut map, "claim", "Rust is faster");
    add(&mut map, "fact", "benchmarks");
    link(&mut map, "backs", ("fact", "benchmarks"), ("claim", "Rust is faster"));

    let names: Vec<&str> = roots(&map).into_iter().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["Rust is faster"]);
}

#[test]
fn a_claimed_node_is_not_a_root() {
    let mut map = Map::empty(map_id("debates"), debates());
    add(&mut map, "verdict", "Go");
    add(&mut map, "verdict", "Rust");
    link(&mut map, "replaces", ("verdict", "Rust"), ("verdict", "Go"));

    let names: Vec<&str> = roots(&map).into_iter().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["Rust"]);
}

#[test]
fn heads_fall_back_to_every_node_when_every_claim_cycles() {
    // Two chores blocking each other claim one another, so neither is
    // a root - but the map is not empty, and what a reader opens it on
    // cannot be nothing.
    let mut map = Map::empty(map_id("chores"), chores());
    add(&mut map, "chore", "A");
    add(&mut map, "chore", "B");
    link(&mut map, "blocks", ("chore", "A"), ("chore", "B"));
    link(&mut map, "blocks", ("chore", "B"), ("chore", "A"));

    assert!(roots(&map).is_empty());
    let names: Vec<&str> = heads(&map).into_iter().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["A", "B"]);
}

#[test]
fn heads_are_the_roots_when_the_map_has_any() {
    let mut map = Map::empty(map_id("debates"), debates());
    add(&mut map, "topic", "Which language?");
    add(&mut map, "claim", "Rust is faster");
    link(&mut map, "about", ("claim", "Rust is faster"), ("topic", "Which language?"));

    let names: Vec<&str> = heads(&map).into_iter().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["Which language?"]);
}

#[test]
fn a_node_pointing_at_two_kinds_nests_under_the_one_declared_later() {
    let mut map = Map::empty(crate::core::MapId::new(), court());
    add(&mut map, "area", "parsing");
    add(&mut map, "topic", "Which parser?");
    add(&mut map, "verdict", "ship it");
    link(&mut map, "within", ("verdict", "ship it"), ("area", "parsing"));
    link(&mut map, "settles", ("verdict", "ship it"), ("topic", "Which parser?"));

    let verdict = map.find("verdict", "ship it").unwrap();

    assert_eq!(claimant(&map, verdict).unwrap().name, "Which parser?");
}
