use std::collections::BTreeMap;

use super::*;
use crate::core::testing::{chores, debates, human, link, map_id};
use crate::core::{Actor, Map, Mutation};

/// Adds a node, with whatever properties its kind carries: a `why`
/// when it declares one, and its closed list's first value when it has
/// one - `claim` and `chore` both declare `why`, and `chore` declares a
/// `state` closed list.
fn add(map: &mut Map, kind: &str, name: &str) {
    let mut properties = BTreeMap::new();
    if let Some(node_kind) = map.schema().node_kind(kind) {
        if node_kind.property("why").is_some() {
            properties.insert("why".to_string(), "because".to_string());
        }
        if let Some((property, values)) = node_kind.closed_list() {
            properties.insert(property.to_string(), values[0].clone());
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
fn a_node_no_edge_reaches_is_a_root() {
    let mut map = Map::empty(map_id("debates"), debates());
    add(&mut map, "topic", "Which language?");

    let names: Vec<&str> = roots(&map).into_iter().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["Which language?"]);
}

#[test]
fn a_node_an_edge_reaches_hangs_under_it_instead() {
    let mut map = Map::empty(map_id("debates"), debates());
    add(&mut map, "topic", "Which language?");
    add(&mut map, "claim", "Rust is faster");
    link(&mut map, "about", ("topic", "Which language?"), ("claim", "Rust is faster"));

    let names: Vec<&str> = roots(&map).into_iter().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["Which language?"]);
}

#[test]
fn every_node_of_a_map_with_no_edges_is_a_root() {
    let mut map = Map::empty(map_id("chores"), chores());
    add(&mut map, "chore", "A");
    add(&mut map, "chore", "B");

    let names: Vec<&str> = roots(&map).into_iter().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["A", "B"]);
}

#[test]
fn a_map_that_holds_a_node_always_has_a_root() {
    // A forest cannot swallow every node in a cycle, so there is no
    // map with nodes and no head.
    let mut map = Map::empty(map_id("chores"), chores());
    add(&mut map, "chore", "A");
    add(&mut map, "chore", "B");
    link(&mut map, "blocks", ("chore", "A"), ("chore", "B"));

    assert_eq!(roots(&map).len(), 1);
}
