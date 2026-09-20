use super::*;
use crate::core::testing::{chores, created_at, debates, files, human, source};
use crate::core::Actor;

mod fold;
mod mutation;
mod reading;
mod selection;

fn committed(payload: Payload) -> Event {
    Event::new(Actor::Human(human()), source("test"), None, payload)
}

/// A `node.added` event, numbered `1` - every call site here adds at
/// most one node of its kind to its map, so the mint order `apply`
/// would use collapses to that one number.
fn node_added(map: &str, node: NodeId, kind: &str, name: &str) -> Event {
    node_added_seq(map, node, kind, name, 1)
}

/// `node_added`, numbered `seq` - for a test whose map holds more than
/// one node of a kind, where each needs the number `apply` would have
/// minted for it: `1`, then `2`, and so on.
fn node_added_seq(map: &str, node: NodeId, kind: &str, name: &str, seq: u32) -> Event {
    node_added_with_properties(map, node, kind, name, BTreeMap::new(), seq)
}

fn node_added_with_properties(
    map: &str,
    node: NodeId,
    kind: &str,
    name: &str,
    properties: BTreeMap<String, String>,
    seq: u32,
) -> Event {
    committed(Payload::NodeAdded {
        map: crate::core::testing::map_id(map),
        node,
        kind: kind.to_string(),
        name: name.to_string(),
        properties,
        sources: Vec::new(),
        seq,
    })
}

fn edge_added(map: &str, kind: &str, from: NodeId, to: NodeId) -> Event {
    committed(Payload::EdgeAdded {
        map: crate::core::testing::map_id(map),
        kind: kind.to_string(),
        from,
        to,
        sources: Vec::new(),
    })
}

fn node_changed(
    map: &str,
    node: NodeId,
    name: Option<&str>,
    properties: BTreeMap<String, String>,
) -> Event {
    committed(Payload::NodeChanged {
        map: crate::core::testing::map_id(map),
        node,
        name: name.map(str::to_string),
        properties,
        sources: Vec::new(),
    })
}

fn node_removed(map: &str, node: NodeId) -> Event {
    committed(Payload::NodeRemoved {
        map: crate::core::testing::map_id(map),
        node,
        sources: Vec::new(),
    })
}

/// A settled debate: a topic, a claim, a verdict,
/// and the edge that settles the topic.
fn rust_over_go() -> ([NodeId; 3], Vec<Event>) {
    let ids = [NodeId::new(), NodeId::new(), NodeId::new()];
    let events = vec![
        node_added("debates", ids[0], "topic", "Which language?"),
        node_added("debates", ids[1], "claim", "Rust"),
        node_added("debates", ids[2], "verdict", "Rust over Go"),
        edge_added("debates", "settles", ids[0], ids[2]),
    ];
    (ids, events)
}

fn node_ref(kind: &str, name: &str) -> NodeRef {
    NodeRef {
        kind: kind.to_string(),
        name: name.to_string(),
    }
}

fn add_node(kind: &str, name: &str) -> Mutation {
    Mutation::AddNode {
        kind: kind.to_string(),
        name: name.to_string(),
        properties: BTreeMap::new(),
        sources: Vec::new(),
    }
}

/// A `topic` node - `topic` carries no states, so `add_node` alone
/// would do; kept as its own helper so a caller reads `add_topic`
/// beside `add_claim` and `add_chore`.
fn add_topic(name: &str) -> Mutation {
    add_node("topic", name)
}

/// A `claim` node with a `why` set, though its kind, carrying only
/// free-text properties, requires none.
fn add_claim(name: &str) -> Mutation {
    Mutation::AddNode {
        kind: "claim".to_string(),
        name: name.to_string(),
        properties: BTreeMap::from([("why".to_string(), "because".to_string())]),
        sources: Vec::new(),
    }
}

fn add_edge(kind: &str, from: NodeRef, to: NodeRef) -> Mutation {
    Mutation::AddEdge {
        kind: kind.to_string(),
        from,
        to,
        sources: Vec::new(),
    }
}

/// A `chore` node with a `why` set and the `state` `Map::apply`
/// requires on add for any kind whose closed list is `state`.
fn add_chore(name: &str) -> Mutation {
    Mutation::AddNode {
        kind: "chore".to_string(),
        name: name.to_string(),
        properties: BTreeMap::from([
            ("why".to_string(), "because".to_string()),
            ("state".to_string(), "open".to_string()),
        ]),
        sources: Vec::new(),
    }
}

fn change_node(
    kind: &str,
    name: &str,
    rename: Option<&str>,
    properties: BTreeMap<String, String>,
) -> Mutation {
    Mutation::ChangeNode {
        node: node_ref(kind, name),
        name: rename.map(str::to_string),
        properties,
        sources: Vec::new(),
    }
}

fn rejected_with(err: MapError, expected: EventId) -> MapError {
    match err {
        MapError::Rejected { event, error } => {
            assert!(event == expected);
            *error
        }
        other => panic!("expected Rejected, got {other}"),
    }
}

/// A chain: topic -> verdict, topic -> claim -> fact, plus
/// an unlinked claim, so depth walks one step at a time, along and
/// against the edge direction, and a genuinely unlinked node stays
/// out at any depth. `settles`, `about`, and `backs` are the only
/// edge kinds that can build it, since `Map::apply` now refuses an
/// edge whose ends are not of the kinds its edge kind declares.
fn chain() -> Map {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_topic("Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("verdict", "Rust over Go"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("fact", "Built both"), Actor::Human(human()))
        .unwrap();
    map.apply(add_claim("Go"), Actor::Human(human())).unwrap();
    map.apply(add_claim("Java"), Actor::Human(human())).unwrap();
    map.apply(
        add_edge(
            "settles",
            node_ref("topic", "Which language?"),
            node_ref("verdict", "Rust over Go"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(
        add_edge(
            "about",
            node_ref("topic", "Which language?"),
            node_ref("claim", "Go"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(
        add_edge(
            "backs",
            node_ref("claim", "Go"),
            node_ref("fact", "Built both"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map
}
