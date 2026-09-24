use super::*;

#[test]
fn a_map_keeps_the_identity_it_was_built_with() {
    let id = MapId::new();

    assert_eq!(Map::empty(id, debates()).id(), id);
}

#[test]
fn fold_uses_map_identity_not_schema_name() {
    let kept = MapId::new();
    let other = MapId::new();
    let events = [
        committed(Payload::NodeAdded {
            map: kept,
            node: NodeId::new(),
            kind: "verdict".to_string(),
            name: "kept".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
            seq: 1,
        }),
        committed(Payload::NodeAdded {
            map: other,
            node: NodeId::new(),
            kind: "verdict".to_string(),
            name: "other".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
            seq: 1,
        }),
    ];

    let map = Map::fold(kept, debates(), &events).unwrap();

    assert!(map.find("verdict", "kept").is_some());
    assert!(map.find("verdict", "other").is_none());
}

#[test]
fn schemas_fold_only_maps_the_log_created() {
    let events = [Event::map_created(
        crate::core::testing::map_id("debates"),
        "debates".to_string(),
        source("init"),
    )];

    let maps = crate::core::testing::schemas().fold_all(&events, &events).unwrap();

    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].schema().name(), "debates");
}

#[test]
fn two_identities_for_one_schema_are_rejected() {
    let first = MapId::new();
    let second = MapId::new();
    let events = [
        Event::map_created(first, "debates".to_string(), source("init")),
        Event::map_created(second, "debates".to_string(), source("init")),
    ];

    let err = match crate::core::testing::schemas().fold_all(&events, &events) {
        Err(err) => err,
        Ok(_) => panic!("expected duplicate map identities to be rejected"),
    };

    assert!(matches!(
        err,
        MapError::DuplicateMapIdentity {
            name,
            first: found_first,
            second: found_second,
        } if name == "debates" && found_first == first && found_second == second
    ));
}

#[test]
fn fold_stamps_a_node_with_its_events_actor_and_time() {
    let event = Event::new(
        Actor::Agent,
        source("test"),
        None,
        Payload::NodeAdded {
            map: crate::core::testing::map_id("debates"),
            node: NodeId::new(),
            kind: "claim".to_string(),
            name: "Rust".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
            seq: 1,
        },
    );
    let created_at = event.created_at();

    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &[event]).unwrap();

    let node = map.find("claim", "Rust").unwrap();
    assert_eq!(node.added().actor, Actor::Agent);
    assert_eq!(node.added().at, created_at);
}

#[test]
fn a_fold_holds_every_node_and_edge_still_present() {
    let (ids, events) = rust_over_go();

    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
    assert_eq!(map.edges().len(), 1);
    assert!(map.find("verdict", "Rust over Go").unwrap().id == ids[2]);
    assert!(map.edges()[0].from == ids[0]);
    assert!(map.edges()[0].to == ids[2]);
}

#[test]
fn a_fold_skips_other_maps_and_other_kinds() {
    let (_, mut events) = rust_over_go();
    events.push(committed(Payload::MessageReceived {
        content: "hi".to_string(),
    }));
    events.push(node_added("chores", NodeId::new(), "goal", "Ship"));

    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
}

#[test]
fn removing_a_node_drops_its_edges() {
    let (ids, mut events) = rust_over_go();
    events.push(node_removed("debates", ids[0]));

    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    assert_eq!(map.nodes().len(), 2);
    assert!(map.edges().is_empty());
}

#[test]
fn removing_an_edge_leaves_its_nodes() {
    let (ids, mut events) = rust_over_go();
    events.push(committed(Payload::EdgeRemoved {
        map: crate::core::testing::map_id("debates"),
        kind: "settles".to_string(),
        from: ids[0],
        to: ids[2],
        sources: Vec::new(),
    }));

    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
    assert!(map.edges().is_empty());
}

#[test]
fn an_unknown_kind_fails_the_fold() {
    let stray = node_added("debates", NodeId::new(), "goal", "Ship");
    let stray_id = stray.id();

    let err = Map::fold(crate::core::testing::map_id("debates"), debates(), &[stray])
        .err()
        .unwrap();

    assert_eq!(
        rejected_with(err, stray_id),
        MapError::UnknownNodeKind {
            map: "debates".to_string(),
            kinds: debates().node_kinds_csv(),
            kind: "goal".to_string()
        }
    );
    assert_eq!(
        MapError::UnknownNodeKind {
            map: "debates".to_string(),
            kinds: debates().node_kinds_csv(),
            kind: "goal".to_string()
        }
        .to_string(),
        "no node kind \"goal\" in map \"debates\"; kinds are `topic`, `claim` \
         (carries `why`, `summary`), `fact` (carries `summary`, `when`), \
         `verdict` (carries `why`)"
    );
}

#[test]
fn a_blank_name_fails_the_fold() {
    let stray = node_added("debates", NodeId::new(), "claim", " ");
    let stray_id = stray.id();

    let err = Map::fold(crate::core::testing::map_id("debates"), debates(), &[stray])
        .err()
        .unwrap();

    assert_eq!(rejected_with(err, stray_id), MapError::BlankName);
}

#[test]
fn a_name_is_unique_within_its_kind_only() {
    let events = vec![
        node_added("debates", NodeId::new(), "claim", "Rust"),
        node_added("debates", NodeId::new(), "verdict", "Rust"),
    ];
    assert_eq!(
        Map::fold(crate::core::testing::map_id("debates"), debates(), &events)
            .unwrap()
            .nodes()
            .len(),
        2
    );

    let twice = node_added("debates", NodeId::new(), "claim", "Rust");
    let twice_id = twice.id();
    let mut events = events;
    events.push(twice);

    let err = Map::fold(crate::core::testing::map_id("debates"), debates(), &events)
        .err()
        .unwrap();

    assert_eq!(
        rejected_with(err, twice_id),
        MapError::DuplicateNode {
            kind: "claim".to_string(),
            name: "Rust".to_string()
        }
    );
}

#[test]
fn an_edge_needs_both_ends_and_is_stated_once() {
    let (ids, mut events) = rust_over_go();
    let dangling = edge_added("debates", "backs", ids[1], NodeId::new());
    let dangling_id = dangling.id();
    let mut with_dangling = events.clone();
    with_dangling.push(dangling);

    let err = Map::fold(
        crate::core::testing::map_id("debates"),
        debates(),
        &with_dangling,
    )
    .err()
    .unwrap();
    assert!(matches!(
        rejected_with(err, dangling_id),
        MapError::NoSuchNodeId(_)
    ));

    let twice = edge_added("debates", "settles", ids[0], ids[2]);
    let twice_id = twice.id();
    events.push(twice);

    let err = Map::fold(crate::core::testing::map_id("debates"), debates(), &events)
        .err()
        .unwrap();
    assert_eq!(
        rejected_with(err, twice_id),
        MapError::DuplicateEdge {
            kind: "settles".to_string(),
            from: "topic \"Which language?\"".to_string(),
            to: "verdict \"Rust over Go\"".to_string()
        }
    );
}

#[test]
fn removing_an_edge_that_is_not_there_fails_the_fold() {
    let (ids, mut events) = rust_over_go();
    let stray = committed(Payload::EdgeRemoved {
        map: crate::core::testing::map_id("debates"),
        kind: "backs".to_string(),
        from: ids[1],
        to: ids[0],
        sources: Vec::new(),
    });
    let stray_id = stray.id();
    events.push(stray);

    let err = Map::fold(crate::core::testing::map_id("debates"), debates(), &events)
        .err()
        .unwrap();

    assert!(matches!(
        rejected_with(err, stray_id),
        MapError::NoSuchEdge { .. }
    ));
}

#[test]
fn a_stored_node_carrying_an_undeclared_property_still_folds() {
    let event = node_added_with_properties(
        "debates",
        NodeId::new(),
        "claim",
        "Rust",
        BTreeMap::from([
            ("why".to_string(), "because".to_string()),
            (
                "candidate".to_string(),
                "a paragraph nobody may write anymore".to_string(),
            ),
        ]),
        1,
    );

    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &[event]).unwrap();

    let node = map.find("claim", "Rust").unwrap();
    assert_eq!(
        node.properties.get("candidate").unwrap(),
        "a paragraph nobody may write anymore"
    );
}

#[test]
fn a_fold_still_accepts_an_edge_between_the_wrong_kinds_from_history() {
    // `check_edge_ends` runs only from `apply`; `replay`, which `fold`
    // uses, never calls it, so an edge recorded before its kind
    // declared ends - or written by a race `apply` did not see - still
    // folds rather than breaking every read of the map.
    let (topic, claim) = (NodeId::new(), NodeId::new());
    let events = [
        node_added("debates", topic, "topic", "Which language?"),
        node_added("debates", claim, "claim", "Go"),
        edge_added("debates", "settles", topic, claim),
    ];

    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    assert_eq!(map.edges().len(), 1);
}

#[test]
fn fold_keeps_the_seq_the_event_recorded_rather_than_the_nodes_position() {
    let events = [
        node_added_seq("debates", NodeId::new(), "verdict", "A", 5),
        node_added_seq("debates", NodeId::new(), "verdict", "B", 9),
    ];

    let map = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    let a = map.find("verdict", "A").unwrap().id;
    let b = map.find("verdict", "B").unwrap().id;
    assert_eq!(map.short_id(a), Some("v5".to_string()));
    assert_eq!(map.short_id(b), Some("v9".to_string()));
}
