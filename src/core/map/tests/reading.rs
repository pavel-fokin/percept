use super::*;

#[test]
fn a_chain_of_edges_reads_as_children_and_a_single_root() {
    let (a, b, c) = (NodeId::new(), NodeId::new(), NodeId::new());
    let events = [
        node_added_seq("debates", a, "verdict", "A", 1),
        node_added_seq("debates", b, "verdict", "B", 2),
        node_added_seq("debates", c, "verdict", "C", 3),
        edge_added("debates", "replaces", b, a),
        edge_added("debates", "replaces", c, b),
    ];
    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    // c replaces b replaces a: each edge runs from the parent (the
    // replacer) to the child (the replaced), so `c` heads the chain
    // and `a` hangs deepest.
    assert_eq!(
        map.children(c)
            .iter()
            .map(|(_, node)| node.id)
            .collect::<Vec<_>>(),
        vec![b]
    );
    assert_eq!(
        map.children(b)
            .iter()
            .map(|(_, node)| node.id)
            .collect::<Vec<_>>(),
        vec![a]
    );
    assert_eq!(map.roots().map(|node| node.id).collect::<Vec<_>>(), vec![c]);
}

#[test]
fn an_edge_reads_as_a_child_of_its_from_end_and_a_root_of_its_to_end() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();
    map.apply(add_chore("cancellable streams"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge(
            "blocks",
            node_ref("chore", "cancellable streams"),
            node_ref("chore", "cancel a turn"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let blocker = map.find("chore", "cancellable streams").unwrap().id;
    let blocked = map.find("chore", "cancel a turn").unwrap().id;

    let under_blocker: Vec<NodeId> = map
        .children(blocker)
        .iter()
        .map(|(_, node)| node.id)
        .collect();
    assert_eq!(under_blocker, vec![blocked]);
    assert!(map.children(blocked).is_empty());
    assert_eq!(
        map.roots().map(|node| node.id).collect::<Vec<_>>(),
        vec![blocker]
    );
}

#[test]
fn roots_are_the_nodes_no_edge_reaches() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("a"), Actor::Human(human())).unwrap();
    map.apply(add_chore("b"), Actor::Human(human())).unwrap();
    map.apply(
        add_edge("blocks", node_ref("chore", "a"), node_ref("chore", "b")),
        Actor::Human(human()),
    )
    .unwrap();

    let names: Vec<&str> = map.roots().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["a"]);
}

#[test]
fn a_node_with_two_parents_is_not_a_root() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("a"), Actor::Human(human())).unwrap();
    map.apply(add_chore("b"), Actor::Human(human())).unwrap();
    map.apply(add_chore("c"), Actor::Human(human())).unwrap();
    map.apply(
        add_edge("blocks", node_ref("chore", "a"), node_ref("chore", "c")),
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(
        add_edge("blocks", node_ref("chore", "b"), node_ref("chore", "c")),
        Actor::Human(human()),
    )
    .unwrap();

    let mut names: Vec<&str> = map.roots().map(|node| node.name.as_str()).collect();
    names.sort_unstable();

    assert_eq!(names, ["a", "b"]);
}

#[test]
fn a_node_no_edge_touches_is_its_own_root() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("alone"), Actor::Human(human()))
        .unwrap();

    let names: Vec<&str> = map.roots().map(|node| node.name.as_str()).collect();

    assert_eq!(names, ["alone"]);
}

#[test]
fn a_cycle_leaves_no_root() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("a"), Actor::Human(human())).unwrap();
    map.apply(add_chore("b"), Actor::Human(human())).unwrap();
    map.apply(
        add_edge("blocks", node_ref("chore", "a"), node_ref("chore", "b")),
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(
        add_edge("blocks", node_ref("chore", "b"), node_ref("chore", "a")),
        Actor::Human(human()),
    )
    .unwrap();

    assert!(map.roots().next().is_none());
}

#[test]
fn a_map_reads_as_one_line_per_node_then_per_edge() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_topic("Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(
        Mutation::AddNode {
            kind: "fact".to_string(),
            name: "Built both".to_string(),
            properties: BTreeMap::from([
                ("summary".to_string(), "side by\nside".to_string()),
                ("when".to_string(), "August".to_string()),
            ]),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(add_node("verdict", "Rust over Go"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge(
            "settles",
            node_ref("topic", "Which language?"),
            node_ref("verdict", "Rust over Go"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    assert_eq!(
        map.to_string(),
        "- topic \"Which language?\"\n\
         - fact \"Built both\": summary: \"side by\\nside\"; when: \"August\"\n\
         - verdict \"Rust over Go\"\n\
         - topic \"Which language?\" settles verdict \"Rust over Go\"\n"
    );
    assert_eq!(
        Map::empty(crate::core::testing::map_id("debates"), debates()).to_string(),
        ""
    );
}

#[test]
fn a_missing_node_names_same_kind_nodes_that_share_a_word() {
    let err = chain()
        .around(&node_ref("verdict", "Rust and Go"), 1)
        .err()
        .unwrap();
    assert_eq!(
        err.to_string(),
        "no verdict \"Rust and Go\" in the map; did you mean verdict:Rust over Go"
    );
}

#[test]
fn a_missing_node_stays_silent_on_a_single_shared_word() {
    let err = chain()
        .around(&node_ref("verdict", "Rust and Java"), 1)
        .err()
        .unwrap();
    assert_eq!(err.to_string(), "no verdict \"Rust and Java\" in the map");
}

#[test]
fn a_missing_node_counts_a_repeated_word_once() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "safe safe pick"), Actor::Human(human()))
        .unwrap();
    let err = map
        .around(&node_ref("verdict", "safe choice"), 1)
        .err()
        .unwrap();
    assert_eq!(err.to_string(), "no verdict \"safe choice\" in the map");
}

#[test]
fn a_missing_node_names_a_matching_node_of_another_kind() {
    let err = chain()
        .around(&node_ref("topic", "Rust over Go"), 1)
        .err()
        .unwrap();
    assert_eq!(
        err.to_string(),
        "no topic \"Rust over Go\" in the map; did you mean verdict:Rust over Go"
    );
}

#[test]
fn a_missing_node_crosses_kinds_on_a_lone_shared_path_segment() {
    let mut map = Map::empty(crate::core::testing::map_id("files"), files());
    map.apply(add_node("file", "src/providers/catalog.rs"), Actor::System)
        .unwrap();
    map.apply(add_node("file", "src/providers/openai.rs"), Actor::System)
        .unwrap();

    let err = map
        .around(&node_ref("package", "providers"), 1)
        .err()
        .unwrap();

    assert_eq!(
        err.to_string(),
        "no package \"providers\" in the map; \
         did you mean file:src/providers/catalog.rs, file:src/providers/openai.rs"
    );
}

#[test]
fn a_missing_prose_node_does_not_cross_kinds_on_one_shared_word() {
    let err = chain()
        .around(&node_ref("topic", "Rust and Kotlin"), 1)
        .err()
        .unwrap();
    assert_eq!(err.to_string(), "no topic \"Rust and Kotlin\" in the map");
}

#[test]
fn a_short_id_is_its_kind_s_prefix_and_its_mint_order() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_topic("Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("verdict", "Rust"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("verdict", "Go"), Actor::Human(human()))
        .unwrap();

    let topic = map.find("topic", "Which language?").unwrap().id;
    let rust = map.find("verdict", "Rust").unwrap().id;
    let go = map.find("verdict", "Go").unwrap().id;

    assert_eq!(map.short_id(topic), Some("t1".to_string()));
    assert_eq!(map.short_id(rust), Some("v1".to_string()));
    assert_eq!(map.short_id(go), Some("v2".to_string()));
}

/// `next_seq_by_kind` tracks a high-water mark per kind, never rolled
/// back on `NodeRemoved` - a short id is cited in chat, PR comments,
/// and committed text, so a removed node's number must never come back
/// under a different node.
#[test]
fn a_removed_node_s_number_is_never_reused() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Go"), Actor::Human(human()))
        .unwrap();
    map.apply(
        Mutation::RemoveNode {
            node: node_ref("verdict", "Go"),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(add_node("verdict", "Rust"), Actor::Human(human()))
        .unwrap();

    let rust = map.find("verdict", "Rust").unwrap().id;

    assert_eq!(map.short_id(rust), Some("v2".to_string()));
}

#[test]
fn resolve_str_takes_a_short_id_or_a_kind_and_name() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Rust over Go"), Actor::Human(human()))
        .unwrap();
    let id = map.find("verdict", "Rust over Go").unwrap().id;

    assert_eq!(map.resolve_str("v1").unwrap(), id);
    assert_eq!(map.resolve_str("verdict:Rust over Go").unwrap(), id);
}

#[test]
fn resolve_str_rejects_an_unknown_prefix_or_number() {
    let map = chain();

    assert!(matches!(
        map.resolve_str("z1"),
        Err(MapError::UnknownShortId(s)) if s == "z1"
    ));
    assert!(matches!(
        map.resolve_str("v99"),
        Err(MapError::UnknownShortId(s)) if s == "v99"
    ));
}
