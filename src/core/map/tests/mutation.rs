use super::*;

#[test]
fn apply_records_what_a_fold_rebuilds() {
    let mut built = Map::empty(crate::core::testing::map_id("debates"), debates());
    let events: Vec<Event> = vec![
        add_topic("Which language?"),
        add_node("verdict", "Rust over Go"),
        add_edge(
            "settles",
            node_ref("topic", "Which language?"),
            node_ref("verdict", "Rust over Go"),
        ),
    ]
    .into_iter()
    .map(|m| committed(built.apply(m, Actor::Human(human())).unwrap()))
    .collect();

    let folded = Map::fold(crate::core::testing::map_id("debates"), debates(), &events).unwrap();

    let verdict = folded.find("verdict", "Rust over Go").unwrap();
    assert!(verdict.id == built.find("verdict", "Rust over Go").unwrap().id);
    assert_eq!(folded.edges().len(), 1);
    assert!(folded.edges()[0].to == verdict.id);
}

#[test]
fn apply_stamps_the_node_with_the_actor_given() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    let me = crate::core::testing::human();
    map.apply(add_claim("Rust"), Actor::Human(me)).unwrap();

    let node = map.find("claim", "Rust").unwrap();

    assert_eq!(node.added().actor, Actor::Human(me));
}

#[test]
fn apply_refuses_a_mutation_and_leaves_the_map_as_it_was() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_claim("Rust"), Actor::Human(human())).unwrap();

    let unknown = map
        .apply(add_node("goal", "Ship"), Actor::Human(human()))
        .err()
        .unwrap();
    let blank = map
        .apply(add_claim("  "), Actor::Human(human()))
        .err()
        .unwrap();
    let duplicate = map
        .apply(add_claim("Rust"), Actor::Human(human()))
        .err()
        .unwrap();
    let missing = map
        .apply(
            add_edge("backs", node_ref("fact", "Nope"), node_ref("claim", "Rust")),
            Actor::Human(human()),
        )
        .err()
        .unwrap();
    let no_edge = map
        .apply(
            Mutation::RemoveEdge {
                kind: "backs".to_string(),
                from: node_ref("claim", "Rust"),
                to: node_ref("claim", "Rust"),
                sources: Vec::new(),
            },
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert!(matches!(unknown, MapError::UnknownNodeKind { .. }));
    assert_eq!(blank, MapError::BlankName);
    assert!(matches!(duplicate, MapError::DuplicateNode { .. }));
    assert!(matches!(
        &missing,
        MapError::NoSuchNode { node, suggestions }
            if node == &node_ref("fact", "Nope") && suggestions.is_empty()
    ));
    assert_eq!(missing.to_string(), "no fact \"Nope\" in the map");
    assert!(matches!(no_edge, MapError::NoSuchEdge { .. }));
    assert_eq!(map.nodes().len(), 1);
    assert!(map.edges().is_empty());
}

#[test]
fn change_merges_one_property_and_keeps_the_rest() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            None,
            BTreeMap::from([("state".to_string(), "done".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let node = map.find("chore", "cancel a turn").unwrap();
    assert_eq!(node.properties.get("why").unwrap(), "because");
    assert_eq!(node.properties.get("state").unwrap(), "done");
}

#[test]
fn a_rename_updates_lookup_by_the_new_name_and_frees_the_old() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            Some("cancel a turn cleanly"),
            BTreeMap::new(),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    assert!(map.find("chore", "cancel a turn").is_none());
    assert!(map.find("chore", "cancel a turn cleanly").is_some());
}

#[test]
fn a_rename_to_a_taken_name_is_refused() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("a"), Actor::Human(human())).unwrap();
    map.apply(add_chore("b"), Actor::Human(human())).unwrap();

    let err = map
        .apply(
            change_node("chore", "a", Some("b"), BTreeMap::new()),
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::DuplicateNode {
            kind: "chore".to_string(),
            name: "b".to_string(),
        }
    );
}

#[test]
fn a_value_off_the_closed_list_is_refused_on_add_and_on_change() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    let values = vec![
        "open".to_string(),
        "done".to_string(),
        "dropped".to_string(),
    ];

    let on_add = map
        .apply(
            Mutation::AddNode {
                kind: "chore".to_string(),
                name: "a".to_string(),
                properties: BTreeMap::from([
                    ("why".to_string(), "because".to_string()),
                    ("state".to_string(), "urgent".to_string()),
                ]),
                sources: Vec::new(),
            },
            Actor::Human(human()),
        )
        .err()
        .unwrap();
    assert_eq!(
        on_add,
        MapError::UnknownValue {
            kind: "chore".to_string(),
            property: "state".to_string(),
            value: "urgent".to_string(),
            values: values.clone(),
        }
    );

    map.apply(add_chore("a"), Actor::Human(human())).unwrap();
    let on_change = map
        .apply(
            change_node(
                "chore",
                "a",
                None,
                BTreeMap::from([("state".to_string(), "urgent".to_string())]),
            ),
            Actor::Human(human()),
        )
        .err()
        .unwrap();
    assert_eq!(
        on_change,
        MapError::UnknownValue {
            kind: "chore".to_string(),
            property: "state".to_string(),
            value: "urgent".to_string(),
            values,
        }
    );
}

#[test]
fn an_undeclared_property_is_refused_on_add() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());

    let err = map
        .apply(
            Mutation::AddNode {
                kind: "claim".to_string(),
                name: "Rust".to_string(),
                properties: BTreeMap::from([
                    ("why".to_string(), "because".to_string()),
                    ("notes".to_string(), "a stray property".to_string()),
                ]),
                sources: Vec::new(),
            },
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::UnknownProperty {
            kind: "claim".to_string(),
            property: "notes".to_string(),
            allowed: vec!["why".to_string(), "summary".to_string()],
        }
    );
    assert_eq!(
        err.to_string(),
        "claim has no property \"notes\"; properties are why, summary"
    );
}

#[test]
fn an_undeclared_property_is_refused_on_change() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_claim("Rust"), Actor::Human(human())).unwrap();

    let err = map
        .apply(
            change_node(
                "claim",
                "Rust",
                None,
                BTreeMap::from([("notes".to_string(), "a stray property".to_string())]),
            ),
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::UnknownProperty {
            kind: "claim".to_string(),
            property: "notes".to_string(),
            allowed: vec!["why".to_string(), "summary".to_string()],
        }
    );
}

#[test]
fn a_required_property_and_a_declared_one_are_accepted() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());

    map.apply(
        Mutation::AddNode {
            kind: "claim".to_string(),
            name: "Rust".to_string(),
            properties: BTreeMap::from([
                ("why".to_string(), "because".to_string()),
                ("summary".to_string(), "fast".to_string()),
            ]),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();

    let node = map.find("claim", "Rust").unwrap();
    assert_eq!(node.properties.get("summary").unwrap(), "fast");
}

#[test]
fn agent_renaming_the_humans_node_is_refused() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    let err = map
        .apply(
            change_node("chore", "cancel a turn", Some("renamed"), BTreeMap::new()),
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn agent_removing_the_humans_node_is_refused() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    let err = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("chore", "cancel a turn"),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn agent_setting_state_on_the_humans_node_is_refused() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    let err = map
        .apply(
            change_node(
                "chore",
                "cancel a turn",
                None,
                BTreeMap::from([("state".to_string(), "done".to_string())]),
            ),
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn agent_adding_an_edge_to_the_humans_node_succeeds() {
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
        Actor::Agent,
    )
    .unwrap();

    assert_eq!(map.edges().len(), 1);
}

#[test]
fn agent_removing_the_humans_edge_is_refused() {
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

    let err = map
        .apply(
            Mutation::RemoveEdge {
                kind: "blocks".to_string(),
                from: node_ref("chore", "cancellable streams"),
                to: node_ref("chore", "cancel a turn"),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn human_may_rename_change_and_remove_the_agents_node() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Agent).unwrap();

    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            Some("cancel a turn cleanly"),
            BTreeMap::from([("why".to_string(), "clearer".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    let node = map.find("chore", "cancel a turn cleanly").unwrap();
    assert_eq!(node.properties.get("why").unwrap(), "clearer");

    map.apply(
        Mutation::RemoveNode {
            node: node_ref("chore", "cancel a turn cleanly"),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
    assert!(map.find("chore", "cancel a turn cleanly").is_none());
}

#[test]
fn after_the_humans_why_the_agent_may_not_rename_or_remove_the_node() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Agent).unwrap();
    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            None,
            BTreeMap::from([("why".to_string(), "wrong".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let renamed = map
        .apply(
            change_node("chore", "cancel a turn", Some("renamed"), BTreeMap::new()),
            Actor::Agent,
        )
        .err()
        .unwrap();
    assert!(matches!(renamed, MapError::NotYours { .. }), "{renamed}");

    let removed = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("chore", "cancel a turn"),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();
    assert!(matches!(removed, MapError::NotYours { .. }), "{removed}");
}

#[test]
fn after_the_humans_why_the_agent_may_not_set_the_nodes_state_either() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Agent).unwrap();
    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            None,
            BTreeMap::from([("why".to_string(), "wrong".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let err = map
        .apply(
            change_node(
                "chore",
                "cancel a turn",
                None,
                BTreeMap::from([("state".to_string(), "done".to_string())]),
            ),
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn an_agent_may_not_cite_a_source_onto_a_node_the_human_wrote() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Rust"), Actor::Human(human()))
        .unwrap();

    let err = map
        .apply(
            Mutation::ChangeNode {
                node: node_ref("verdict", "Rust"),
                name: None,
                properties: BTreeMap::new(),
                sources: vec![EventId::new()],
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn a_change_becomes_the_nodes_last_change() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    let me = human();
    map.apply(add_chore("cancel a turn"), Actor::Agent).unwrap();
    let added_at = map.find("chore", "cancel a turn").unwrap().changed().at;

    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            None,
            BTreeMap::from([("why".to_string(), "wrong".to_string())]),
        ),
        Actor::Human(me),
    )
    .unwrap();

    let node = map.find("chore", "cancel a turn").unwrap();
    assert_eq!(node.changed().actor, Actor::Human(me));
    assert!(node.changed().at >= added_at);
}

#[test]
fn a_change_naming_nothing_is_refused() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent)
        .unwrap();

    let err = map
        .apply(
            change_node("verdict", "Rust", None, BTreeMap::new()),
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::EmptyChange), "{err}");
}

#[test]
fn a_change_citing_only_a_source_the_node_already_has_is_refused() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent)
        .unwrap();
    let drawn_from = EventId::new();
    map.apply(
        Mutation::ChangeNode {
            node: node_ref("verdict", "Rust"),
            name: None,
            properties: BTreeMap::new(),
            sources: vec![drawn_from],
        },
        Actor::Agent,
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::ChangeNode {
                node: node_ref("verdict", "Rust"),
                name: None,
                properties: BTreeMap::new(),
                sources: vec![drawn_from],
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::EmptyChange), "{err}");
}

#[test]
fn a_change_carrying_only_sources_joins_them_to_the_node() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent)
        .unwrap();
    let drawn_from = EventId::new();

    map.apply(
        Mutation::ChangeNode {
            node: node_ref("verdict", "Rust"),
            name: None,
            properties: BTreeMap::new(),
            sources: vec![drawn_from],
        },
        Actor::Agent,
    )
    .unwrap();

    assert!(map
        .find("verdict", "Rust")
        .unwrap()
        .sources
        .contains(&drawn_from));
}

#[test]
fn an_added_nodes_only_change_is_its_addition() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent)
        .unwrap();

    let node = map.find("verdict", "Rust").unwrap();
    assert_eq!(node.changed().actor, Actor::Agent);
    assert_eq!(node.history.len(), 1);
}

#[test]
fn adding_a_node_of_a_kind_with_a_closed_list_and_no_value_writes_none() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());

    map.apply(
        Mutation::AddNode {
            kind: "chore".to_string(),
            name: "a".to_string(),
            properties: BTreeMap::from([("why".to_string(), "because".to_string())]),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();

    let node = map.find("chore", "a").unwrap();
    assert_eq!(node.properties.get("state"), None);
    assert_eq!(map.property(node, "state"), Some("open"));
}

#[test]
fn adding_a_node_with_a_listed_state_succeeds() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());

    map.apply(add_chore("a"), Actor::Human(human())).unwrap();

    assert_eq!(
        map.find("chore", "a")
            .unwrap()
            .properties
            .get("state")
            .unwrap(),
        "open"
    );
}

#[test]
fn a_second_edge_into_one_node_is_accepted() {
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

    assert_eq!(map.edges().len(), 2);
}

#[test]
fn an_edge_closing_a_cycle_is_accepted() {
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

    assert_eq!(map.edges().len(), 2);
    assert!(map.roots().next().is_none(), "the cycle leaves no root");
}

#[test]
fn an_edge_from_a_node_to_itself_is_accepted() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("a"), Actor::Human(human())).unwrap();

    map.apply(
        add_edge("blocks", node_ref("chore", "a"), node_ref("chore", "a")),
        Actor::Human(human()),
    )
    .unwrap();

    assert_eq!(map.edges().len(), 1);
}

#[test]
fn a_human_may_change_anything_on_a_humans_node() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            Some("cancel a turn cleanly"),
            BTreeMap::from([("why".to_string(), "different".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let node = map.find("chore", "cancel a turn cleanly").unwrap();
    assert_eq!(node.properties.get("why").unwrap(), "different");
}

#[test]
fn apply_refuses_an_edge_whose_from_node_is_the_wrong_kind() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_claim("Go"), Actor::Human(human())).unwrap();
    map.apply(add_node("verdict", "Rust over Go"), Actor::Human(human()))
        .unwrap();

    let err = map
        .apply(
            add_edge(
                "settles",
                node_ref("claim", "Go"),
                node_ref("verdict", "Rust over Go"),
            ),
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::WrongEdgeEnd {
            edge_kind: "settles".to_string(),
            end: EdgeEnd::From,
            allowed: vec!["topic".to_string()],
            found: "claim".to_string(),
        }
    );
    assert!(map.edges().is_empty());
}

#[test]
fn apply_removes_a_node_by_name_and_its_edges_with_it() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_topic("Which language?"), Actor::Human(human()))
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

    let payload = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("topic", "Which language?"),
                sources: Vec::new(),
            },
            Actor::Human(human()),
        )
        .unwrap();

    assert!(matches!(payload, Payload::NodeRemoved { .. }));
    assert_eq!(map.nodes().len(), 1);
    assert!(map.edges().is_empty());
}

#[test]
fn removing_a_node_a_humans_edge_touches_is_refused_to_the_agent() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_claim("Rust"), Actor::Agent).unwrap();
    map.apply(add_topic("Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge(
            "about",
            node_ref("topic", "Which language?"),
            node_ref("claim", "Rust"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("claim", "Rust"),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn removing_the_agents_own_edge_off_a_node_the_human_touched_is_refused() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent)
        .unwrap();
    map.apply(add_topic("Which language?"), Actor::Agent)
        .unwrap();
    map.apply(
        add_edge(
            "settles",
            node_ref("topic", "Which language?"),
            node_ref("verdict", "Rust"),
        ),
        Actor::Agent,
    )
    .unwrap();
    map.apply(
        change_node(
            "verdict",
            "Rust",
            None,
            BTreeMap::from([("why".to_string(), "does not settle it".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::RemoveEdge {
                kind: "settles".to_string(),
                from: node_ref("topic", "Which language?"),
                to: node_ref("verdict", "Rust"),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn removing_the_agents_own_node_is_refused_while_its_edge_hangs_on_a_node_the_human_touched() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent)
        .unwrap();
    map.apply(add_topic("Which language?"), Actor::Agent)
        .unwrap();
    map.apply(
        add_edge(
            "settles",
            node_ref("topic", "Which language?"),
            node_ref("verdict", "Rust"),
        ),
        Actor::Agent,
    )
    .unwrap();
    map.apply(
        change_node(
            "topic",
            "Which language?",
            Some("Which language, still open?"),
            BTreeMap::new(),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("verdict", "Rust"),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}
