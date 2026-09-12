use super::*;
use crate::core::testing::{chores, created_at, debates, files, human, source};
use crate::core::Actor;

fn committed(payload: Payload) -> Event {
    Event::new(Actor::Human(human()), source("test"), None, payload)
}

#[test]
fn default_prefix_is_the_name_s_first_letter_lowercased() {
    assert_eq!(default_prefix("Verdict"), "v");
    assert_eq!(default_prefix("chore"), "c");
}

#[test]
fn kind_new_defaults_its_prefix() {
    assert_eq!(NodeKind::new("fact", "g").prefix, "f");
}

#[test]
fn headlines_are_the_schema_s_headline_kinds_in_map_order() {
    let events = [
        node_added("debates", NodeId::new(), "claim", "Go"),
        node_added("debates", NodeId::new(), "topic", "Which language?"),
        node_added("debates", NodeId::new(), "verdict", "Rust"),
    ];
    let map = Map::fold(debates(), &events).unwrap();
    let names: Vec<&str> = map.headlines().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["Which language?", "Rust"]);
}

#[test]
fn a_replaced_verdict_still_shows_in_the_headlines() {
    // The core keeps no notion of "replaced": a headline is any node
    // of a headline kind, full stop. A reader tells the two apart by
    // the `replaces` edge itself, not by one dropping out.
    let (old, new) = (NodeId::new(), NodeId::new());
    let events = [
        node_added("debates", old, "verdict", "Go"),
        node_added("debates", new, "verdict", "Rust"),
        edge_added("debates", "replaces", new, old),
    ];
    let map = Map::fold(debates(), &events).unwrap();
    let names: Vec<&str> = map.headlines().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["Go", "Rust"]);
}

#[test]
fn linked_follows_an_edge_kind_the_core_names_no_meaning_for() {
    let (a, b, c) = (NodeId::new(), NodeId::new(), NodeId::new());
    let events = [
        node_added("debates", a, "verdict", "A"),
        node_added("debates", b, "verdict", "B"),
        node_added("debates", c, "verdict", "C"),
        edge_added("debates", "replaces", b, a),
        edge_added("debates", "replaces", c, b),
    ];
    let map = Map::fold(debates(), &events).unwrap();

    assert_eq!(
        map.linked(a, "replaces", EdgeEnd::To)
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        vec![b]
    );
    assert_eq!(
        map.linked(c, "replaces", EdgeEnd::From)
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        vec![b]
    );
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
        created_at(edge_added("debates", "settles", new, topic), at),
    ];
    let map = Map::fold(debates(), &events).unwrap();

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
        created_at(
            edge_added("debates", "settles", verdict, topic),
            earlier,
        ),
    ];
    let map = Map::fold(debates(), &events).unwrap();

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
    let map = Map::fold(chores(), &events).unwrap();

    let cut = map.since(at);

    assert_eq!(cut.nodes().len(), 1);
}

fn node_added(map: &str, node: NodeId, kind: &str, name: &str) -> Event {
    // `seq` at its sentinel: these fixtures build a few nodes at most,
    // so `Map::replay`'s positional fallback mints the same numbers a
    // fresh `apply` would, and every test here is about something else.
    committed(Payload::NodeAdded {
        map: map.to_string(),
        node,
        kind: kind.to_string(),
        name: name.to_string(),
        properties: BTreeMap::new(),
        sources: Vec::new(),
        seq: 0,
    })
}

fn edge_added(map: &str, kind: &str, from: NodeId, to: NodeId) -> Event {
    committed(Payload::EdgeAdded {
        map: map.to_string(),
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
        map: map.to_string(),
        node,
        name: name.map(str::to_string),
        properties,
        sources: Vec::new(),
        why: None,
    })
}

fn node_removed(map: &str, node: NodeId) -> Event {
    committed(Payload::NodeRemoved {
        map: map.to_string(),
        node,
        why: "gone".to_string(),
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
        edge_added("debates", "settles", ids[2], ids[0]),
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

/// A `claim` node with the `why` its kind requires - `add_node`
/// leaves `properties` empty, which `Map::apply` now refuses for a
/// kind that requires one.
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

/// A `chore` node with the `why` its kind requires and the `state`
/// `Map::apply` now requires on add for any kind that declares one.
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

fn change_node(kind: &str, name: &str, rename: Option<&str>, properties: BTreeMap<String, String>) -> Mutation {
    change_node_why(kind, name, rename, properties, None)
}

/// `change_node`, also carrying `why` - for a test about the comment
/// change W6 always allows.
fn change_node_why(
    kind: &str,
    name: &str,
    rename: Option<&str>,
    properties: BTreeMap<String, String>,
    why: Option<&str>,
) -> Mutation {
    Mutation::ChangeNode {
        node: node_ref(kind, name),
        name: rename.map(str::to_string),
        properties,
        sources: Vec::new(),
        why: why.map(str::to_string),
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

#[test]
fn fold_stamps_a_node_with_its_events_actor_and_time() {
    let event = Event::new(
        Actor::Agent,
        source("test"),
        None,
        Payload::NodeAdded {
            map: "debates".to_string(),
            node: NodeId::new(),
            kind: "claim".to_string(),
            name: "Rust".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
            seq: 0,
        },
    );
    let created_at = event.created_at();

    let map = Map::fold(debates(), &[event]).unwrap();

    let node = map.find("claim", "Rust").unwrap();
    assert_eq!(node.added().actor, Actor::Agent);
    assert_eq!(node.added().at, created_at);
}

#[test]
fn a_fold_holds_every_node_and_edge_still_present() {
    let (ids, events) = rust_over_go();

    let map = Map::fold(debates(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
    assert_eq!(map.edges().len(), 1);
    assert!(map.find("verdict", "Rust over Go").unwrap().id == ids[2]);
    assert!(map.edges()[0].from == ids[2]);
    assert!(map.edges()[0].to == ids[0]);
}

#[test]
fn a_fold_skips_other_maps_and_other_kinds() {
    let (_, mut events) = rust_over_go();
    events.push(committed(Payload::MessageReceived {
        content: "hi".to_string(),
    }));
    events.push(node_added("chores", NodeId::new(), "goal", "Ship"));

    let map = Map::fold(debates(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
}

#[test]
fn removing_a_node_drops_its_edges() {
    let (ids, mut events) = rust_over_go();
    events.push(node_removed("debates", ids[0]));

    let map = Map::fold(debates(), &events).unwrap();

    assert_eq!(map.nodes().len(), 2);
    assert!(map.edges().is_empty());
}

#[test]
fn removing_an_edge_leaves_its_nodes() {
    let (ids, mut events) = rust_over_go();
    events.push(committed(Payload::EdgeRemoved {
        map: "debates".to_string(),
        kind: "settles".to_string(),
        from: ids[2],
        to: ids[0],
        sources: Vec::new(),
        why: "answered".to_string(),
    }));

    let map = Map::fold(debates(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
    assert!(map.edges().is_empty());
}

#[test]
fn an_unknown_kind_fails_the_fold() {
    let stray = node_added("debates", NodeId::new(), "goal", "Ship");
    let stray_id = stray.id();

    let err = Map::fold(debates(), &[stray]).err().unwrap();

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
         (requires `why`), `fact`, `verdict`"
    );
}

#[test]
fn a_blank_name_fails_the_fold() {
    let stray = node_added("debates", NodeId::new(), "claim", " ");
    let stray_id = stray.id();

    let err = Map::fold(debates(), &[stray]).err().unwrap();

    assert_eq!(rejected_with(err, stray_id), MapError::BlankName);
}

#[test]
fn a_name_is_unique_within_its_kind_only() {
    let events = vec![
        node_added("debates", NodeId::new(), "claim", "Rust"),
        node_added("debates", NodeId::new(), "verdict", "Rust"),
    ];
    assert_eq!(
        Map::fold(debates(), &events)
            .unwrap()
            .nodes()
            .len(),
        2
    );

    let twice = node_added("debates", NodeId::new(), "claim", "Rust");
    let twice_id = twice.id();
    let mut events = events;
    events.push(twice);

    let err = Map::fold(debates(), &events).err().unwrap();

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
    let dangling = edge_added("debates", "backs", NodeId::new(), ids[1]);
    let dangling_id = dangling.id();
    let mut with_dangling = events.clone();
    with_dangling.push(dangling);

    let err = Map::fold(debates(), &with_dangling)
        .err()
        .unwrap();
    assert!(matches!(
        rejected_with(err, dangling_id),
        MapError::NoSuchNodeId(_)
    ));

    let twice = edge_added("debates", "settles", ids[2], ids[0]);
    let twice_id = twice.id();
    events.push(twice);

    let err = Map::fold(debates(), &events).err().unwrap();
    assert_eq!(
        rejected_with(err, twice_id),
        MapError::DuplicateEdge {
            kind: "settles".to_string(),
            from: "verdict \"Rust over Go\"".to_string(),
            to: "topic \"Which language?\"".to_string()
        }
    );
}

#[test]
fn removing_an_edge_that_is_not_there_fails_the_fold() {
    let (ids, mut events) = rust_over_go();
    let stray = committed(Payload::EdgeRemoved {
        map: "debates".to_string(),
        kind: "backs".to_string(),
        from: ids[1],
        to: ids[0],
        sources: Vec::new(),
        why: "gone".to_string(),
    });
    let stray_id = stray.id();
    events.push(stray);

    let err = Map::fold(debates(), &events).err().unwrap();

    assert!(matches!(
        rejected_with(err, stray_id),
        MapError::NoSuchEdge { .. }
    ));
}

#[test]
fn apply_records_what_a_fold_rebuilds() {
    let mut built = Map::empty(debates());
    let events: Vec<Event> = vec![
        add_topic("Which language?"),
        add_node("verdict", "Rust over Go"),
        add_edge(
            "settles",
            node_ref("verdict", "Rust over Go"),
            node_ref("topic", "Which language?"),
        ),
    ]
    .into_iter()
    .map(|m| committed(built.apply(m, Actor::Human(human())).unwrap()))
    .collect();

    let folded = Map::fold(debates(), &events).unwrap();

    let verdict = folded.find("verdict", "Rust over Go").unwrap();
    assert!(verdict.id == built.find("verdict", "Rust over Go").unwrap().id);
    assert_eq!(folded.edges().len(), 1);
    assert!(folded.edges()[0].from == verdict.id);
}

#[test]
fn apply_stamps_the_node_with_the_actor_given() {
    let mut map = Map::empty(debates());
    let me = crate::core::testing::human();
    map.apply(add_claim("Rust"), Actor::Human(me)).unwrap();

    let node = map.find("claim", "Rust").unwrap();

    assert_eq!(node.added().actor, Actor::Human(me));
}

#[test]
fn apply_refuses_a_claim_without_a_why() {
    let mut map = Map::empty(debates());

    let err = map
        .apply(add_node("claim", "Rust"), Actor::Human(human()))
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::MissingProperty {
            kind: "claim".to_string(),
            name: "Rust".to_string(),
            property: "why".to_string(),
            gloss: debates().node_kind("claim").unwrap().gloss.clone(),
        }
    );
    assert!(map.nodes().is_empty());
}

#[test]
fn apply_refuses_a_mutation_and_leaves_the_map_as_it_was() {
    let mut map = Map::empty(debates());
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
            add_edge(
                "backs",
                node_ref("fact", "Nope"),
                node_ref("claim", "Rust"),
            ),
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
                why: "gone".to_string(),
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
    let mut map = Map::empty(chores());
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
    let mut map = Map::empty(chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node("chore", "cancel a turn", Some("cancel a turn cleanly"), BTreeMap::new()),
        Actor::Human(human()),
    )
    .unwrap();

    assert!(map.find("chore", "cancel a turn").is_none());
    assert!(map.find("chore", "cancel a turn cleanly").is_some());
}

#[test]
fn a_rename_to_a_taken_name_is_refused() {
    let mut map = Map::empty(chores());
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
fn a_state_off_the_list_is_refused_on_add_and_on_change() {
    let mut map = Map::empty(chores());
    let states = vec!["open".to_string(), "done".to_string(), "dropped".to_string()];

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
        MapError::UnknownState {
            kind: "chore".to_string(),
            value: "urgent".to_string(),
            states: states.clone(),
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
        MapError::UnknownState {
            kind: "chore".to_string(),
            value: "urgent".to_string(),
            states,
        }
    );
}

#[test]
fn a_state_on_a_kind_with_no_states_is_refused() {
    let mut map = Map::empty(debates());

    let err = map
        .apply(
            Mutation::AddNode {
                kind: "claim".to_string(),
                name: "Rust".to_string(),
                properties: BTreeMap::from([
                    ("why".to_string(), "because".to_string()),
                    ("state".to_string(), "open".to_string()),
                ]),
                sources: Vec::new(),
            },
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::UnknownState {
            kind: "claim".to_string(),
            value: "open".to_string(),
            states: Vec::new(),
        }
    );
}

#[test]
fn agent_renaming_the_humans_node_is_refused() {
    let mut map = Map::empty(chores());
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
    let mut map = Map::empty(chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    let err = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("chore", "cancel a turn"),
                why: "not needed".to_string(),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn agent_setting_state_on_the_humans_node_succeeds() {
    let mut map = Map::empty(chores());
    map.apply(add_chore("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            None,
            BTreeMap::from([("state".to_string(), "done".to_string())]),
        ),
        Actor::Agent,
    )
    .unwrap();

    let node = map.find("chore", "cancel a turn").unwrap();
    assert_eq!(node.properties.get("state").unwrap(), "done");
}

#[test]
fn agent_adding_an_edge_to_the_humans_node_succeeds() {
    let mut map = Map::empty(chores());
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
    let mut map = Map::empty(chores());
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
                why: "not needed".to_string(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn human_may_rename_change_and_remove_the_agents_node() {
    let mut map = Map::empty(chores());
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
            why: "not needed".to_string(),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
    assert!(map.find("chore", "cancel a turn cleanly").is_none());
}

#[test]
fn after_the_humans_why_the_agent_may_not_rename_or_remove_the_node() {
    let mut map = Map::empty(chores());
    map.apply(add_chore("cancel a turn"), Actor::Agent).unwrap();
    map.apply(
        change_node_why("chore", "cancel a turn", None, BTreeMap::new(), Some("wrong")),
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
                why: "not needed".to_string(),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();
    assert!(matches!(removed, MapError::NotYours { .. }), "{removed}");
}

#[test]
fn after_the_humans_why_the_agent_may_still_set_the_nodes_state() {
    let mut map = Map::empty(chores());
    map.apply(add_chore("cancel a turn"), Actor::Agent).unwrap();
    map.apply(
        change_node_why("chore", "cancel a turn", None, BTreeMap::new(), Some("wrong")),
        Actor::Human(human()),
    )
    .unwrap();

    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            None,
            BTreeMap::from([("state".to_string(), "done".to_string())]),
        ),
        Actor::Agent,
    )
    .unwrap();

    let node = map.find("chore", "cancel a turn").unwrap();
    assert_eq!(node.properties.get("state").unwrap(), "done");
}

#[test]
fn a_change_carrying_only_why_becomes_the_nodes_last_change() {
    let mut map = Map::empty(chores());
    let me = human();
    map.apply(add_chore("cancel a turn"), Actor::Agent).unwrap();
    let added_at = map.find("chore", "cancel a turn").unwrap().changed().at;

    map.apply(
        change_node_why("chore", "cancel a turn", None, BTreeMap::new(), Some("wrong")),
        Actor::Human(me),
    )
    .unwrap();

    let node = map.find("chore", "cancel a turn").unwrap();
    assert_eq!(node.changed().actor, Actor::Human(me));
    assert_eq!(node.changed().why.as_deref(), Some("wrong"));
    assert!(node.changed().at >= added_at);
}

#[test]
fn an_added_nodes_only_change_is_its_addition_with_no_why() {
    let mut map = Map::empty(debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent).unwrap();

    let node = map.find("verdict", "Rust").unwrap();
    assert_eq!(node.changed().actor, Actor::Agent);
    assert_eq!(node.changed().why, None);
}

#[test]
fn adding_a_node_of_a_kind_with_states_and_no_state_is_refused() {
    let mut map = Map::empty(chores());

    let err = map
        .apply(
            Mutation::AddNode {
                kind: "chore".to_string(),
                name: "a".to_string(),
                properties: BTreeMap::from([("why".to_string(), "because".to_string())]),
                sources: Vec::new(),
            },
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::MissingState {
            kind: "chore".to_string(),
            states: vec!["open".to_string(), "done".to_string(), "dropped".to_string()],
        }
    );
    assert_eq!(err.to_string(), "chore needs a state; states are open, done, dropped");
}

#[test]
fn adding_a_node_with_a_listed_state_succeeds() {
    let mut map = Map::empty(chores());

    map.apply(add_chore("a"), Actor::Human(human())).unwrap();

    assert_eq!(
        map.find("chore", "a").unwrap().properties.get("state").unwrap(),
        "open"
    );
}

#[test]
fn linked_reads_in_both_directions() {
    let mut map = Map::empty(chores());
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

    let from_blocker: Vec<NodeId> = map
        .linked(blocker, "blocks", EdgeEnd::From)
        .iter()
        .map(|node| node.id)
        .collect();
    assert_eq!(from_blocker, vec![blocked]);

    let to_blocked: Vec<NodeId> = map
        .linked(blocked, "blocks", EdgeEnd::To)
        .iter()
        .map(|node| node.id)
        .collect();
    assert_eq!(to_blocked, vec![blocker]);

    assert!(map.linked(blocked, "blocks", EdgeEnd::From).is_empty());
    assert!(map.linked(blocker, "blocks", EdgeEnd::To).is_empty());
}

#[test]
fn a_human_may_change_anything_on_a_humans_node() {
    let mut map = Map::empty(chores());
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
    let mut map = Map::empty(debates());
    map.apply(add_topic("Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_claim("Go"), Actor::Human(human())).unwrap();

    let err = map
        .apply(
            add_edge(
                "settles",
                node_ref("claim", "Go"),
                node_ref("topic", "Which language?"),
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
            allowed: vec!["verdict".to_string()],
            found: "claim".to_string(),
        }
    );
    assert!(map.edges().is_empty());
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
        edge_added("debates", "settles", claim, topic),
    ];

    let map = Map::fold(debates(), &events).unwrap();

    assert_eq!(map.edges().len(), 1);
}

#[test]
fn apply_removes_a_node_by_name_and_its_edges_with_it() {
    let mut map = Map::empty(debates());
    map.apply(add_topic("Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("verdict", "Rust over Go"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge(
            "settles",
            node_ref("verdict", "Rust over Go"),
            node_ref("topic", "Which language?"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let payload = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("topic", "Which language?"),
                why: "answered".to_string(),
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
fn a_map_reads_as_one_line_per_node_then_per_edge() {
    let mut map = Map::empty(debates());
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
            node_ref("verdict", "Rust over Go"),
            node_ref("topic", "Which language?"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    assert_eq!(
        map.to_string(),
        "- topic \"Which language?\"\n\
         - fact \"Built both\": summary: \"side by\\nside\"; when: \"August\"\n\
         - verdict \"Rust over Go\"\n\
         - verdict \"Rust over Go\" settles topic \"Which language?\"\n"
    );
    assert_eq!(Map::empty(debates()).to_string(), "");
}

#[test]
fn an_unknown_map_with_no_schemas_says_none_is_declared() {
    let schemas = Schemas::new(Vec::new());
    assert_eq!(
        schemas.find("decisions").err().unwrap().to_string(),
        "no map named \"decisions\"; no map is declared"
    );
}

#[test]
fn a_schema_is_found_by_name() {
    let schemas = crate::core::testing::schemas();
    assert_eq!(schemas.find("debates").unwrap().name, "debates");
    assert_eq!(
        schemas.find("glossary").err().unwrap().to_string(),
        "no map named \"glossary\"; maps are debates, chores"
    );
}

#[test]
fn a_topic_requires_nothing() {
    assert!(debates().node_kind("topic").unwrap().requires.is_empty());
}

#[test]
fn an_undeclared_kind_is_absent() {
    assert!(debates().node_kind("glossary").is_none());
}

#[test]
fn keeping_kinds_drops_other_nodes_and_the_edges_that_touched_them() {
    let (_, events) = rust_over_go();
    let map = Map::fold(debates(), &events).unwrap();

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
    let map = Map::empty(debates());
    let err = map.keep_kinds(&["goal".to_string()]).err().unwrap();
    assert!(matches!(err, MapError::UnknownNodeKind { .. }));
}

/// A chain: topic <- verdict, topic <- claim <- fact, plus
/// an unlinked claim, so depth walks one step at a time, against the
/// edge direction, and a genuinely unlinked node stays out at any
/// depth. `settles`, `about`, and `backs` are the only edge
/// kinds that can build it, since `Map::apply` now refuses an edge
/// whose ends are not of the kinds its edge kind declares.
fn chain() -> Map {
    let mut map = Map::empty(debates());
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
            node_ref("verdict", "Rust over Go"),
            node_ref("topic", "Which language?"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(
        add_edge(
            "about",
            node_ref("claim", "Go"),
            node_ref("topic", "Which language?"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(
        add_edge(
            "backs",
            node_ref("fact", "Built both"),
            node_ref("claim", "Go"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map
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

    let fragment = Map::empty(debates()).select(&selection).unwrap();

    assert_eq!(fragment.total_nodes(), 0);
    assert!(fragment.map().nodes().is_empty());
}

#[test]
fn since_on_a_log_folded_map_is_fine() {
    let selection = Selection {
        since: Some(Timestamp::now()),
        ..Selection::default()
    };

    assert!(Map::empty(debates()).select(&selection).is_ok());
}

#[test]
fn select_walks_around_before_it_keeps_kinds() {
    let topic = node_ref("topic", "Which language?");
    let kinds = ["fact".to_string()];
    let selection = Selection {
        around: Some((&topic, 2)),
        since: None,
        kinds: &kinds,
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

#[test]
fn around_a_node_the_map_lacks_is_an_error() {
    let err = chain()
        .around(&node_ref("claim", "Rust"), 1)
        .err()
        .unwrap();
    assert!(matches!(
        &err,
        MapError::NoSuchNode { node, .. } if node == &node_ref("claim", "Rust")
    ));
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
    let mut map = Map::empty(debates());
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
    let mut map = Map::empty(files());
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
    assert_eq!(
        err.to_string(),
        "no topic \"Rust and Kotlin\" in the map"
    );
}

#[test]
fn a_short_id_is_its_kind_s_prefix_and_its_mint_order() {
    let mut map = Map::empty(debates());
    map.apply(add_topic("Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("verdict", "Rust"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("verdict", "Go"), Actor::Human(human())).unwrap();

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
    let mut map = Map::empty(debates());
    map.apply(add_node("verdict", "Go"), Actor::Human(human())).unwrap();
    map.apply(
        Mutation::RemoveNode {
            node: node_ref("verdict", "Go"),
            why: "reconsidered".to_string(),
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
    let mut map = Map::empty(debates());
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

#[test]
fn a_node_added_event_with_no_seq_falls_back_to_its_position() {
    let events = [
        node_added("debates", NodeId::new(), "verdict", "A"),
        node_added("debates", NodeId::new(), "verdict", "B"),
    ];

    let map = Map::fold(debates(), &events).unwrap();

    let a = map.find("verdict", "A").unwrap().id;
    let b = map.find("verdict", "B").unwrap().id;
    assert_eq!(map.short_id(a), Some("v1".to_string()));
    assert_eq!(map.short_id(b), Some("v2".to_string()));
}

#[test]
fn a_state_set_from_below_does_not_lift_the_lock_the_humans_change_put_on_a_node() {
    let mut map = Map::empty(chores());
    map.apply(add_chore("cancel a turn"), Actor::Agent).unwrap();
    map.apply(
        change_node_why("chore", "cancel a turn", None, BTreeMap::new(), Some("wrong")),
        Actor::Human(human()),
    )
    .unwrap();

    map.apply(
        change_node(
            "chore",
            "cancel a turn",
            None,
            BTreeMap::from([("state".to_string(), "done".to_string())]),
        ),
        Actor::Agent,
    )
    .unwrap();

    let node = map.find("chore", "cancel a turn").unwrap();
    assert_eq!(node.changed().actor, Actor::Agent);
    assert!(matches!(node.touched_by(), Actor::Human(_)), "{:?}", node.touched_by());
    let renamed = map
        .apply(
            change_node("chore", "cancel a turn", Some("renamed"), BTreeMap::new()),
            Actor::Agent,
        )
        .err()
        .unwrap();
    assert!(matches!(renamed, MapError::NotYours { .. }), "{renamed}");
}

#[test]
fn removing_a_node_a_humans_edge_touches_is_refused_to_the_agent() {
    let mut map = Map::empty(debates());
    map.apply(add_claim("Rust"), Actor::Agent).unwrap();
    map.apply(add_topic("Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge("about", node_ref("claim", "Rust"), node_ref("topic", "Which language?")),
        Actor::Human(human()),
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("claim", "Rust"),
                why: "wrong".to_string(),
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
    let mut map = Map::empty(debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent).unwrap();
    map.apply(add_topic("Which language?"), Actor::Agent)
        .unwrap();
    map.apply(
        add_edge("settles", node_ref("verdict", "Rust"), node_ref("topic", "Which language?")),
        Actor::Agent,
    )
    .unwrap();
    map.apply(
        change_node_why("verdict", "Rust", None, BTreeMap::new(), Some("does not settle it")),
        Actor::Human(human()),
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::RemoveEdge {
                kind: "settles".to_string(),
                from: node_ref("verdict", "Rust"),
                to: node_ref("topic", "Which language?"),
                sources: Vec::new(),
                why: "re-pointing".to_string(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn removing_the_agents_own_node_is_refused_while_its_edge_hangs_on_a_node_the_human_touched() {
    let mut map = Map::empty(debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent).unwrap();
    map.apply(add_topic("Which language?"), Actor::Agent)
        .unwrap();
    map.apply(
        add_edge("settles", node_ref("verdict", "Rust"), node_ref("topic", "Which language?")),
        Actor::Agent,
    )
    .unwrap();
    map.apply(
        change_node_why("topic", "Which language?", None, BTreeMap::new(), Some("still open")),
        Actor::Human(human()),
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("verdict", "Rust"),
                sources: Vec::new(),
                why: "retracting".to_string(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn a_blank_why_is_refused_on_a_change() {
    let mut map = Map::empty(debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent).unwrap();

    let err = map
        .apply(
            change_node_why("verdict", "Rust", None, BTreeMap::new(), Some("  ")),
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::BlankWhy), "{err}");
}

#[test]
fn a_blank_why_is_refused_on_a_node_removal() {
    let mut map = Map::empty(debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent).unwrap();

    let err = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("verdict", "Rust"),
                why: String::new(),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::BlankWhy), "{err}");
}

#[test]
fn a_blank_why_is_refused_on_an_edge_removal() {
    let mut map = Map::empty(debates());
    map.apply(add_node("verdict", "Rust"), Actor::Agent).unwrap();
    map.apply(add_topic("Which language?"), Actor::Agent)
        .unwrap();
    map.apply(
        add_edge("settles", node_ref("verdict", "Rust"), node_ref("topic", "Which language?")),
        Actor::Agent,
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::RemoveEdge {
                kind: "settles".to_string(),
                from: node_ref("verdict", "Rust"),
                to: node_ref("topic", "Which language?"),
                sources: Vec::new(),
                why: " ".to_string(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::BlankWhy), "{err}");
}
