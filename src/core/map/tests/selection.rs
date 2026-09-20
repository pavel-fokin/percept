use super::*;

#[test]
fn around_a_node_the_map_lacks_is_an_error() {
    let err = chain().around(&node_ref("claim", "Rust"), 1).err().unwrap();
    assert!(matches!(
        &err,
        MapError::NoSuchNode { node, .. } if *node == node_ref("claim", "Rust")
    ));
}

#[test]
fn since_keeps_what_was_added_from_that_instant_and_what_it_attached_to() {
    let (topic, old, new) = (NodeId::new(), NodeId::new(), NodeId::new());
    let at = Timestamp::now();
    let earlier = at.minus_minutes(1).unwrap();
    let events = [
        created_at(
            node_added("debates", topic, "topic", "Which language?"),
            earlier,
        ),
        created_at(node_added("debates", old, "claim", "Go"), earlier),
        created_at(node_added("debates", new, "verdict", "Rust"), at),
        created_at(edge_added("debates", "settles", topic, new), at),
    ];
    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    let cut = map.since(at);

    let mut names: Vec<&str> = cut.nodes().iter().map(|node| node.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["Rust", "Which language?"]);
    assert_eq!(cut.edges().len(), 1);
}

#[test]
fn since_leaves_out_an_edge_older_than_the_instant_between_kept_nodes() {
    let (topic, verdict) = (NodeId::new(), NodeId::new());
    let at = Timestamp::now();
    let earlier = at.minus_minutes(1).unwrap();
    let events = [
        created_at(
            node_added("debates", topic, "topic", "Which language?"),
            earlier,
        ),
        created_at(node_added("debates", verdict, "verdict", "Rust"), at),
        created_at(edge_added("debates", "settles", topic, verdict), earlier),
    ];
    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    let cut = map.since(at);

    let names: Vec<&str> = cut.nodes().iter().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["Rust"]);
    assert!(cut.edges().is_empty());
}

#[test]
fn since_includes_a_node_changed_after_at() {
    let t = NodeId::new();
    let at = Timestamp::now();
    let earlier = at.minus_minutes(1).unwrap();
    let events = [
        created_at(node_added("chores", t, "chore", "cancel a turn"), earlier),
        created_at(
            node_changed(
                "chores",
                t,
                None,
                BTreeMap::from([("state".to_string(), "done".to_string())]),
            ),
            at,
        ),
    ];
    let map = Map::fold(crate::core::testing::map_id("chores"), chores(), &events).unwrap();

    let cut = map.since(at);

    assert_eq!(cut.nodes().len(), 1);
}

#[test]
fn keeping_kinds_drops_other_nodes_and_the_edges_that_touched_them() {
    let (_, events) = rust_over_go();
    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    let cut = map.keep_kinds(&["verdict".to_string()]).unwrap();
    assert_eq!(cut.nodes().len(), 1);
    assert!(cut.edges().is_empty());

    let both = map
        .keep_kinds(&["verdict".to_string(), "topic".to_string()])
        .unwrap();
    assert_eq!(both.nodes().len(), 2);
    assert_eq!(both.edges().len(), 1);
    assert_eq!(map.nodes().len(), 3, "the cut is a copy");
}

#[test]
fn keeping_a_kind_the_schema_lacks_is_an_error() {
    let map = Map::empty(crate::core::testing::map_id("debates"), debates());
    let err = map.keep_kinds(&["goal".to_string()]).err().unwrap();
    assert!(matches!(err, MapError::UnknownNodeKind { .. }));
}

#[test]
fn around_at_depth_zero_is_the_node_alone() {
    let cut = chain()
        .around(&node_ref("verdict", "Rust over Go"), 0)
        .unwrap();
    assert_eq!(cut.nodes().len(), 1);
    assert!(cut.edges().is_empty());
}

#[test]
fn around_follows_edges_both_ways_one_step_per_depth() {
    let map = chain();
    let one = map
        .around(&node_ref("topic", "Which language?"), 1)
        .unwrap();
    let names: Vec<&str> = one.nodes().iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["Which language?", "Rust over Go", "Go"]);
    assert_eq!(one.edges().len(), 2);

    let two = map
        .around(&node_ref("topic", "Which language?"), 2)
        .unwrap();
    assert_eq!(two.nodes().len(), 4, "the unlinked claim stays out");
    assert_eq!(two.edges().len(), 3);
}

#[test]
fn select_counts_the_whole_and_the_edges_crossing_the_cut() {
    let whole = chain();
    let (nodes, edges) = (whole.nodes().len(), whole.edges().len());
    let topic = node_ref("topic", "Which language?");
    let selection = Selection {
        around: Some((&topic, 1)),
        ..Selection::default()
    };

    let fragment = whole.select(&selection).unwrap();

    assert_eq!(fragment.map().nodes().len(), 3);
    assert_eq!(fragment.total_nodes(), nodes);
    assert_eq!(fragment.total_edges(), edges);
    assert_eq!(
        fragment.boundary_edges(),
        1,
        "the claim's edge onward to fact"
    );
}

#[test]
fn a_whole_selection_is_the_map_itself() {
    let fragment = chain().select(&Selection::default()).unwrap();

    assert_eq!(fragment.map().nodes().len(), fragment.total_nodes());
    assert_eq!(fragment.map().edges().len(), fragment.total_edges());
    assert_eq!(fragment.boundary_edges(), 0);
}

#[test]
fn select_on_an_empty_map_is_the_empty_map_not_a_missing_node() {
    let topic = node_ref("topic", "Which language?");
    let selection = Selection {
        around: Some((&topic, 1)),
        ..Selection::default()
    };

    let fragment = Map::empty(crate::core::testing::map_id("debates"), debates())
        .select(&selection)
        .unwrap();

    assert_eq!(fragment.total_nodes(), 0);
    assert!(fragment.map().nodes().is_empty());
}

#[test]
fn since_on_a_log_folded_map_is_fine() {
    let selection = Selection {
        since: Some(Timestamp::now()),
        ..Selection::default()
    };

    assert!(
        Map::empty(crate::core::testing::map_id("debates"), debates())
            .select(&selection)
            .is_ok()
    );
}

#[test]
fn select_walks_around_before_it_keeps_kinds() {
    let topic = node_ref("topic", "Which language?");
    let kinds = ["fact".to_string()];
    let selection = Selection {
        around: Some((&topic, 2)),
        since: None,
        kinds: &kinds,
        ..Selection::default()
    };

    let fragment = chain().select(&selection).unwrap();

    let names: Vec<&str> = fragment
        .map()
        .nodes()
        .iter()
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["Built both"],
        "reached through the claim, then kept alone"
    );
    assert_eq!(fragment.boundary_edges(), 1);
}
