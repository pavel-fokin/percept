use super::*;
use crate::core::testing::{
    created_at, decisions, files, human, node_added_at, scope, source, tasks, ROOT,
};
use crate::core::Actor;

fn committed(payload: Payload) -> Event {
    Event::new(Actor::Human(human()), source("test"), None, payload)
}

#[test]
fn default_prefix_is_the_name_s_first_letter_lowercased() {
    assert_eq!(default_prefix("Decision"), "d");
    assert_eq!(default_prefix("task"), "t");
}

#[test]
fn kind_new_defaults_its_prefix() {
    assert_eq!(NodeKind::new("evidence", "g").prefix, "e");
}

#[test]
fn headlines_are_the_schema_s_headline_kinds_in_map_order() {
    let events = [
        node_added("decisions", NodeId::new(), "option", "Go"),
        node_added("decisions", NodeId::new(), "question", "Which language?"),
        node_added("decisions", NodeId::new(), "decision", "Rust"),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();
    let names: Vec<&str> = map.headlines().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["Which language?", "Rust"]);
}

#[test]
fn a_superseded_decision_leaves_the_headlines() {
    let (old, new) = (NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", old, "decision", "Go"),
        node_added("decisions", new, "decision", "Rust"),
        edge_added("decisions", SUPERSEDES, new, old),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();
    let names: Vec<&str> = map.headlines().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["Rust"]);
    assert!(map.is_superseded(old));
}

#[test]
fn successor_follows_a_supersession_chain_to_its_end() {
    let (a, b, c) = (NodeId::new(), NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", a, "decision", "A"),
        node_added("decisions", b, "decision", "B"),
        node_added("decisions", c, "decision", "C"),
        edge_added("decisions", SUPERSEDES, b, a),
        edge_added("decisions", SUPERSEDES, c, b),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert_eq!(map.successor(a), c);
    assert_eq!(map.successor(c), c);
    let names: Vec<&str> = map
        .predecessors(c)
        .iter()
        .map(|node| node.name.as_str())
        .collect();
    assert_eq!(names, ["B", "A"]);
}

#[test]
fn a_question_is_settled_by_the_current_end_of_each_resolvers_chain() {
    let (q, a, b) = (NodeId::new(), NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", q, "question", "Which?"),
        node_added("decisions", a, "decision", "A"),
        node_added("decisions", b, "decision", "B"),
        edge_added("decisions", RESOLVES, a, q),
        edge_added("decisions", SUPERSEDES, b, a),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    let names: Vec<&str> = map
        .settled_by(q)
        .iter()
        .map(|node| node.name.as_str())
        .collect();
    assert_eq!(names, ["B"]);
    assert!(map.settles(a));
    assert!(map.settles(b));
}

#[test]
fn a_task_s_open_list_names_the_ones_it_blocks_on() {
    let (t, blocker) = (NodeId::new(), NodeId::new());
    let events = [
        node_added("tasks", t, "task", "cancel a turn"),
        node_added("tasks", blocker, "task", "cancellable streams"),
        edge_added("tasks", "blocks", blocker, t),
    ];
    let map = Map::fold(tasks(), &scope(), &events).unwrap();

    assert_eq!(
        map.blocked_by(t).iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![blocker]
    );
    assert_eq!(
        map.open().map(|n| n.id).collect::<Vec<_>>(),
        vec![t, blocker]
    );
}

#[test]
fn open_lists_only_the_settled_kind_never_the_settling_one() {
    // decisions' headline_kinds is ["question", "decision"], and
    // nothing ever settles a decision node itself - `open` must filter
    // to the settled kind (`settlement.of`) first, or every decision
    // would misreport as open alongside the real open question.
    let (settled, decision, open) = (NodeId::new(), NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", settled, "question", "settled one"),
        node_added("decisions", decision, "decision", "the answer"),
        edge_added("decisions", RESOLVES, decision, settled),
        node_added("decisions", open, "question", "still open"),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    let names: Vec<&str> = map.open().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["still open"]);
}

#[test]
fn open_is_empty_on_a_map_with_no_settlement() {
    let mut schema = decisions();
    schema.settlement = None;
    let events = [node_added("decisions", NodeId::new(), "question", "Which?")];
    let map = Map::fold(schema, &scope(), &events).unwrap();

    assert_eq!(map.open().count(), 0);
}

#[test]
fn weighed_for_lists_answering_options_but_not_ones_that_restate_the_decision() {
    let (q, lost, restated, d) = (NodeId::new(), NodeId::new(), NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", q, "question", "Which parser?"),
        node_added("decisions", lost, "option", "reuse OpenAi"),
        node_added("decisions", restated, "option", "its own parser"),
        node_added("decisions", d, "decision", "its own parser"),
        edge_added("decisions", ANSWERS, lost, q),
        edge_added("decisions", ANSWERS, restated, q),
        edge_added("decisions", RESOLVES, d, q),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert_eq!(
        map.weighed_for(q).iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![lost]
    );
}

#[test]
fn a_reopening_question_is_listed_under_the_decision_and_names_it() {
    let (q, d, doubt) = (NodeId::new(), NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", q, "question", "Which parser?"),
        node_added("decisions", d, "decision", "its own parser"),
        edge_added("decisions", RESOLVES, d, q),
        node_added("decisions", doubt, "question", "Does its own parser still fit?"),
        edge_added("decisions", REOPENS, doubt, d),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert_eq!(map.reopened_by(d).iter().map(|n| n.id).collect::<Vec<_>>(), vec![doubt]);
    assert_eq!(map.reopens(doubt).iter().map(|n| n.id).collect::<Vec<_>>(), vec![d]);
    assert!(map.reopened_by(q).is_empty());
    assert_eq!(map.settled_by(q).iter().map(|n| n.id).collect::<Vec<_>>(), vec![d]);
}

#[test]
fn a_resolves_edge_between_other_kinds_settles_nothing() {
    let (o, d) = (NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", o, "option", "Go"),
        node_added("decisions", d, "decision", "Rust"),
        edge_added("decisions", RESOLVES, d, o),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert!(map.settled_by(o).is_empty());
    assert!(!map.settles(d));
}

#[test]
fn since_keeps_what_was_added_from_that_instant_and_what_it_attached_to() {
    let (question, old, new) = (NodeId::new(), NodeId::new(), NodeId::new());
    let at = Timestamp::now();
    let earlier = at.minus_minutes(1).unwrap();
    let events = [
        created_at(
            node_added("decisions", question, "question", "Which language?"),
            earlier,
        ),
        created_at(node_added("decisions", old, "option", "Go"), earlier),
        created_at(node_added("decisions", new, "decision", "Rust"), at),
        created_at(edge_added("decisions", "resolves", new, question), at),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    let cut = map.since(at);

    let mut names: Vec<&str> = cut.nodes().iter().map(|node| node.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["Rust", "Which language?"]);
    assert_eq!(cut.edges().len(), 1);
}

#[test]
fn since_leaves_out_an_edge_older_than_the_instant_between_kept_nodes() {
    let (question, decision) = (NodeId::new(), NodeId::new());
    let at = Timestamp::now();
    let earlier = at.minus_minutes(1).unwrap();
    let events = [
        created_at(
            node_added("decisions", question, "question", "Which language?"),
            earlier,
        ),
        created_at(node_added("decisions", decision, "decision", "Rust"), at),
        created_at(
            edge_added("decisions", "resolves", decision, question),
            earlier,
        ),
    ];
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

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
        created_at(node_added("tasks", t, "task", "cancel a turn"), earlier),
        created_at(
            node_changed(
                "tasks",
                t,
                None,
                BTreeMap::from([("state".to_string(), "done".to_string())]),
            ),
            at,
        ),
    ];
    let map = Map::fold(tasks(), &scope(), &events).unwrap();

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

/// The decision from the ADR: a question, an option, a decision,
/// and the edge that resolves the question.
fn rust_over_go() -> ([NodeId; 3], Vec<Event>) {
    let ids = [NodeId::new(), NodeId::new(), NodeId::new()];
    let events = vec![
        node_added("decisions", ids[0], "question", "Which language?"),
        node_added("decisions", ids[1], "option", "Rust"),
        node_added("decisions", ids[2], "decision", "Rust over Go"),
        edge_added("decisions", "resolves", ids[2], ids[0]),
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

/// An `option` node with the `why` its kind requires - `add_node`
/// leaves `properties` empty, which `Map::apply` now refuses for a
/// kind that requires one.
fn add_option(name: &str) -> Mutation {
    Mutation::AddNode {
        kind: "option".to_string(),
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

/// A `task` node with the `why` its kind requires and the `state`
/// `Map::apply` now requires on add for any kind that declares one.
fn add_task(name: &str) -> Mutation {
    Mutation::AddNode {
        kind: "task".to_string(),
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
            map: "decisions".to_string(),
            node: NodeId::new(),
            kind: "option".to_string(),
            name: "Rust".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
            seq: 0,
        },
    );
    let created_at = event.created_at();

    let map = Map::fold(decisions(), &scope(), &[event]).unwrap();

    let node = map.find("option", "Rust").unwrap();
    assert_eq!(node.actor, Actor::Agent);
    assert_eq!(node.added_at, created_at);
}

#[test]
fn a_fold_holds_every_node_and_edge_still_present() {
    let (ids, events) = rust_over_go();

    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
    assert_eq!(map.edges().len(), 1);
    assert!(map.find("decision", "Rust over Go").unwrap().id == ids[2]);
    assert!(map.edges()[0].from == ids[2]);
    assert!(map.edges()[0].to == ids[0]);
}

#[test]
fn a_fold_skips_other_maps_and_other_kinds() {
    let (_, mut events) = rust_over_go();
    events.push(committed(Payload::MessageReceived {
        content: "hi".to_string(),
    }));
    events.push(node_added("tasks", NodeId::new(), "goal", "Ship"));

    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
}

#[test]
fn a_fold_scoped_to_a_project_skips_a_node_added_under_another_path() {
    let events = [
        node_added_at(ROOT, "option", "Rust"),
        node_added_at("/other", "option", "Go"),
    ];

    let scoped = Map::fold(decisions(), &scope(), &events).unwrap();
    assert_eq!(scoped.nodes().len(), 1);
    assert!(scoped.find("option", "Rust").is_some());

    let all = Map::fold(decisions(), &Scope::All, &events).unwrap();
    assert_eq!(all.nodes().len(), 2);
}

#[test]
fn removing_a_node_drops_its_edges() {
    let (ids, mut events) = rust_over_go();
    events.push(node_removed("decisions", ids[0]));

    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert_eq!(map.nodes().len(), 2);
    assert!(map.edges().is_empty());
}

#[test]
fn removing_an_edge_leaves_its_nodes() {
    let (ids, mut events) = rust_over_go();
    events.push(committed(Payload::EdgeRemoved {
        map: "decisions".to_string(),
        kind: "resolves".to_string(),
        from: ids[2],
        to: ids[0],
        sources: Vec::new(),
        why: "answered".to_string(),
    }));

    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
    assert!(map.edges().is_empty());
}

#[test]
fn an_unknown_kind_fails_the_fold() {
    let stray = node_added("decisions", NodeId::new(), "goal", "Ship");
    let stray_id = stray.id();

    let err = Map::fold(decisions(), &scope(), &[stray]).err().unwrap();

    assert_eq!(
        rejected_with(err, stray_id),
        MapError::UnknownNodeKind {
            map: "decisions".to_string(),
            kinds: decisions().node_kinds_csv(),
            kind: "goal".to_string()
        }
    );
    assert_eq!(
        MapError::UnknownNodeKind {
            map: "decisions".to_string(),
            kinds: decisions().node_kinds_csv(),
            kind: "goal".to_string()
        }
        .to_string(),
        "no node kind \"goal\" in map \"decisions\"; kinds are `question`, `option` \
         (requires `why`), `evidence`, `decision`"
    );
}

#[test]
fn a_blank_name_fails_the_fold() {
    let stray = node_added("decisions", NodeId::new(), "option", " ");
    let stray_id = stray.id();

    let err = Map::fold(decisions(), &scope(), &[stray]).err().unwrap();

    assert_eq!(rejected_with(err, stray_id), MapError::BlankName);
}

#[test]
fn a_name_is_unique_within_its_kind_only() {
    let events = vec![
        node_added("decisions", NodeId::new(), "option", "Rust"),
        node_added("decisions", NodeId::new(), "decision", "Rust"),
    ];
    assert_eq!(
        Map::fold(decisions(), &scope(), &events)
            .unwrap()
            .nodes()
            .len(),
        2
    );

    let twice = node_added("decisions", NodeId::new(), "option", "Rust");
    let twice_id = twice.id();
    let mut events = events;
    events.push(twice);

    let err = Map::fold(decisions(), &scope(), &events).err().unwrap();

    assert_eq!(
        rejected_with(err, twice_id),
        MapError::DuplicateNode {
            kind: "option".to_string(),
            name: "Rust".to_string()
        }
    );
}

#[test]
fn an_edge_needs_both_ends_and_is_stated_once() {
    let (ids, mut events) = rust_over_go();
    let dangling = edge_added("decisions", "supports", NodeId::new(), ids[1]);
    let dangling_id = dangling.id();
    let mut with_dangling = events.clone();
    with_dangling.push(dangling);

    let err = Map::fold(decisions(), &scope(), &with_dangling)
        .err()
        .unwrap();
    assert!(matches!(
        rejected_with(err, dangling_id),
        MapError::NoSuchNodeId(_)
    ));

    let twice = edge_added("decisions", "resolves", ids[2], ids[0]);
    let twice_id = twice.id();
    events.push(twice);

    let err = Map::fold(decisions(), &scope(), &events).err().unwrap();
    assert_eq!(
        rejected_with(err, twice_id),
        MapError::DuplicateEdge {
            kind: "resolves".to_string(),
            from: "decision \"Rust over Go\"".to_string(),
            to: "question \"Which language?\"".to_string()
        }
    );
}

#[test]
fn removing_an_edge_that_is_not_there_fails_the_fold() {
    let (ids, mut events) = rust_over_go();
    let stray = committed(Payload::EdgeRemoved {
        map: "decisions".to_string(),
        kind: "supports".to_string(),
        from: ids[1],
        to: ids[0],
        sources: Vec::new(),
        why: "gone".to_string(),
    });
    let stray_id = stray.id();
    events.push(stray);

    let err = Map::fold(decisions(), &scope(), &events).err().unwrap();

    assert!(matches!(
        rejected_with(err, stray_id),
        MapError::NoSuchEdge { .. }
    ));
}

#[test]
fn apply_records_what_a_fold_rebuilds() {
    let mut built = Map::empty(decisions());
    let events: Vec<Event> = vec![
        add_node("question", "Which language?"),
        add_node("decision", "Rust over Go"),
        add_edge(
            "resolves",
            node_ref("decision", "Rust over Go"),
            node_ref("question", "Which language?"),
        ),
    ]
    .into_iter()
    .map(|m| committed(built.apply(m, Actor::Human(human())).unwrap()))
    .collect();

    let folded = Map::fold(decisions(), &scope(), &events).unwrap();

    let decision = folded.find("decision", "Rust over Go").unwrap();
    assert!(decision.id == built.find("decision", "Rust over Go").unwrap().id);
    assert_eq!(folded.edges().len(), 1);
    assert!(folded.edges()[0].from == decision.id);
}

#[test]
fn apply_stamps_the_node_with_the_actor_given() {
    let mut map = Map::empty(decisions());
    let me = crate::core::testing::human();
    map.apply(add_option("Rust"), Actor::Human(me)).unwrap();

    let node = map.find("option", "Rust").unwrap();

    assert_eq!(node.actor, Actor::Human(me));
}

#[test]
fn apply_refuses_an_option_without_a_why() {
    let mut map = Map::empty(decisions());

    let err = map
        .apply(add_node("option", "Rust"), Actor::Human(human()))
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::MissingProperty {
            kind: "option".to_string(),
            name: "Rust".to_string(),
            property: "why".to_string(),
            gloss: decisions().node_kind("option").unwrap().gloss.clone(),
        }
    );
    assert!(map.nodes().is_empty());
}

#[test]
fn apply_refuses_a_mutation_and_leaves_the_map_as_it_was() {
    let mut map = Map::empty(decisions());
    map.apply(add_option("Rust"), Actor::Human(human())).unwrap();

    let unknown = map
        .apply(add_node("goal", "Ship"), Actor::Human(human()))
        .err()
        .unwrap();
    let blank = map
        .apply(add_option("  "), Actor::Human(human()))
        .err()
        .unwrap();
    let duplicate = map
        .apply(add_option("Rust"), Actor::Human(human()))
        .err()
        .unwrap();
    let missing = map
        .apply(
            add_edge(
                "supports",
                node_ref("evidence", "Nope"),
                node_ref("option", "Rust"),
            ),
            Actor::Human(human()),
        )
        .err()
        .unwrap();
    let no_edge = map
        .apply(
            Mutation::RemoveEdge {
                kind: "supports".to_string(),
                from: node_ref("option", "Rust"),
                to: node_ref("option", "Rust"),
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
            if node == &node_ref("evidence", "Nope") && suggestions.is_empty()
    ));
    assert_eq!(missing.to_string(), "no evidence \"Nope\" in the map");
    assert!(matches!(no_edge, MapError::NoSuchEdge { .. }));
    assert_eq!(map.nodes().len(), 1);
    assert!(map.edges().is_empty());
}

#[test]
fn change_merges_one_property_and_keeps_the_rest() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node(
            "task",
            "cancel a turn",
            None,
            BTreeMap::from([("state".to_string(), "done".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let node = map.find("task", "cancel a turn").unwrap();
    assert_eq!(node.properties.get("why").unwrap(), "because");
    assert_eq!(node.properties.get("state").unwrap(), "done");
}

#[test]
fn a_rename_updates_lookup_by_the_new_name_and_frees_the_old() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node("task", "cancel a turn", Some("cancel a turn cleanly"), BTreeMap::new()),
        Actor::Human(human()),
    )
    .unwrap();

    assert!(map.find("task", "cancel a turn").is_none());
    assert!(map.find("task", "cancel a turn cleanly").is_some());
}

#[test]
fn a_rename_to_a_taken_name_is_refused() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("a"), Actor::Human(human())).unwrap();
    map.apply(add_task("b"), Actor::Human(human())).unwrap();

    let err = map
        .apply(
            change_node("task", "a", Some("b"), BTreeMap::new()),
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::DuplicateNode {
            kind: "task".to_string(),
            name: "b".to_string(),
        }
    );
}

#[test]
fn a_state_off_the_list_is_refused_on_add_and_on_change() {
    let mut map = Map::empty(tasks());
    let states = vec!["open".to_string(), "done".to_string(), "dropped".to_string()];

    let on_add = map
        .apply(
            Mutation::AddNode {
                kind: "task".to_string(),
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
            kind: "task".to_string(),
            value: "urgent".to_string(),
            states: states.clone(),
        }
    );

    map.apply(add_task("a"), Actor::Human(human())).unwrap();
    let on_change = map
        .apply(
            change_node(
                "task",
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
            kind: "task".to_string(),
            value: "urgent".to_string(),
            states,
        }
    );
}

#[test]
fn a_state_on_a_kind_with_no_states_is_refused() {
    let mut map = Map::empty(decisions());

    let err = map
        .apply(
            Mutation::AddNode {
                kind: "option".to_string(),
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
            kind: "option".to_string(),
            value: "open".to_string(),
            states: Vec::new(),
        }
    );
}

#[test]
fn agent_renaming_the_humans_node_is_refused() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();

    let err = map
        .apply(
            change_node("task", "cancel a turn", Some("renamed"), BTreeMap::new()),
            Actor::Agent,
        )
        .err()
        .unwrap();

    assert!(matches!(err, MapError::NotYours { .. }), "{err}");
}

#[test]
fn agent_removing_the_humans_node_is_refused() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();

    let err = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("task", "cancel a turn"),
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
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node(
            "task",
            "cancel a turn",
            None,
            BTreeMap::from([("state".to_string(), "done".to_string())]),
        ),
        Actor::Agent,
    )
    .unwrap();

    let node = map.find("task", "cancel a turn").unwrap();
    assert_eq!(node.properties.get("state").unwrap(), "done");
}

#[test]
fn agent_adding_an_edge_to_the_humans_node_succeeds() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();
    map.apply(add_task("cancellable streams"), Actor::Human(human()))
        .unwrap();

    map.apply(
        add_edge(
            "blocks",
            node_ref("task", "cancellable streams"),
            node_ref("task", "cancel a turn"),
        ),
        Actor::Agent,
    )
    .unwrap();

    assert_eq!(map.edges().len(), 1);
}

#[test]
fn agent_removing_the_humans_edge_is_refused() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();
    map.apply(add_task("cancellable streams"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge(
            "blocks",
            node_ref("task", "cancellable streams"),
            node_ref("task", "cancel a turn"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let err = map
        .apply(
            Mutation::RemoveEdge {
                kind: "blocks".to_string(),
                from: node_ref("task", "cancellable streams"),
                to: node_ref("task", "cancel a turn"),
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
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Agent).unwrap();

    map.apply(
        change_node(
            "task",
            "cancel a turn",
            Some("cancel a turn cleanly"),
            BTreeMap::from([("why".to_string(), "clearer".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    let node = map.find("task", "cancel a turn cleanly").unwrap();
    assert_eq!(node.properties.get("why").unwrap(), "clearer");

    map.apply(
        Mutation::RemoveNode {
            node: node_ref("task", "cancel a turn cleanly"),
            why: "not needed".to_string(),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
    assert!(map.find("task", "cancel a turn cleanly").is_none());
}

#[test]
fn after_the_humans_why_the_agent_may_not_rename_or_remove_but_may_still_set_state() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Agent).unwrap();

    map.apply(
        change_node_why("task", "cancel a turn", None, BTreeMap::new(), Some("wrong")),
        Actor::Human(human()),
    )
    .unwrap();

    let renamed = map
        .apply(
            change_node("task", "cancel a turn", Some("renamed"), BTreeMap::new()),
            Actor::Agent,
        )
        .err()
        .unwrap();
    assert!(matches!(renamed, MapError::NotYours { .. }), "{renamed}");

    let removed = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("task", "cancel a turn"),
                why: "not needed".to_string(),
                sources: Vec::new(),
            },
            Actor::Agent,
        )
        .err()
        .unwrap();
    assert!(matches!(removed, MapError::NotYours { .. }), "{removed}");

    map.apply(
        change_node(
            "task",
            "cancel a turn",
            None,
            BTreeMap::from([("state".to_string(), "done".to_string())]),
        ),
        Actor::Agent,
    )
    .unwrap();
    let node = map.find("task", "cancel a turn").unwrap();
    assert_eq!(node.properties.get("state").unwrap(), "done");
}

#[test]
fn a_change_carrying_only_why_updates_changed_by_changed_why_and_changed_at() {
    let mut map = Map::empty(tasks());
    let me = human();
    map.apply(add_task("cancel a turn"), Actor::Agent).unwrap();
    let added_at = map.find("task", "cancel a turn").unwrap().changed_at;

    map.apply(
        change_node_why("task", "cancel a turn", None, BTreeMap::new(), Some("wrong")),
        Actor::Human(me),
    )
    .unwrap();

    let node = map.find("task", "cancel a turn").unwrap();
    assert_eq!(node.changed_by, Actor::Human(me));
    assert_eq!(node.changed_why.as_deref(), Some("wrong"));
    assert!(node.changed_at >= added_at);
}

#[test]
fn node_added_sets_changed_by_to_its_actor_and_changed_why_to_none() {
    let mut map = Map::empty(decisions());
    map.apply(add_node("decision", "Rust"), Actor::Agent).unwrap();

    let node = map.find("decision", "Rust").unwrap();
    assert_eq!(node.changed_by, Actor::Agent);
    assert_eq!(node.changed_why, None);
}

#[test]
fn adding_a_node_of_a_kind_with_states_and_no_state_is_refused() {
    let mut map = Map::empty(tasks());

    let err = map
        .apply(
            Mutation::AddNode {
                kind: "task".to_string(),
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
        MapError::UnknownState {
            kind: "task".to_string(),
            value: String::new(),
            states: vec!["open".to_string(), "done".to_string(), "dropped".to_string()],
        }
    );
    assert_eq!(err.to_string(), "task needs a state; states are open, done, dropped");
}

#[test]
fn adding_a_node_with_a_listed_state_succeeds() {
    let mut map = Map::empty(tasks());

    map.apply(add_task("a"), Actor::Human(human())).unwrap();

    assert_eq!(
        map.find("task", "a").unwrap().properties.get("state").unwrap(),
        "open"
    );
}

#[test]
fn linked_reads_in_both_directions() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();
    map.apply(add_task("cancellable streams"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge(
            "blocks",
            node_ref("task", "cancellable streams"),
            node_ref("task", "cancel a turn"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let blocker = map.find("task", "cancellable streams").unwrap().id;
    let blocked = map.find("task", "cancel a turn").unwrap().id;

    let from_blocker: Vec<NodeId> = map
        .linked(blocker, "blocks", Dir::From)
        .iter()
        .map(|node| node.id)
        .collect();
    assert_eq!(from_blocker, vec![blocked]);

    let to_blocked: Vec<NodeId> = map
        .linked(blocked, "blocks", Dir::To)
        .iter()
        .map(|node| node.id)
        .collect();
    assert_eq!(to_blocked, vec![blocker]);

    assert!(map.linked(blocked, "blocks", Dir::From).is_empty());
    assert!(map.linked(blocker, "blocks", Dir::To).is_empty());
}

#[test]
fn a_human_may_change_anything_on_a_humans_node() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("cancel a turn"), Actor::Human(human()))
        .unwrap();

    map.apply(
        change_node(
            "task",
            "cancel a turn",
            Some("cancel a turn cleanly"),
            BTreeMap::from([("why".to_string(), "different".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let node = map.find("task", "cancel a turn cleanly").unwrap();
    assert_eq!(node.properties.get("why").unwrap(), "different");
}

#[test]
fn open_on_the_tasks_fixture_lists_first_state_tasks_only() {
    let mut map = Map::empty(tasks());
    map.apply(add_task("a"), Actor::Human(human())).unwrap();
    map.apply(add_task("b"), Actor::Human(human())).unwrap();
    map.apply(
        change_node(
            "task",
            "b",
            None,
            BTreeMap::from([("state".to_string(), "done".to_string())]),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let names: Vec<&str> = map.open().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["a"]);
}

#[test]
fn apply_refuses_an_edge_whose_from_node_is_the_wrong_kind() {
    let mut map = Map::empty(decisions());
    map.apply(add_node("question", "Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_option("Go"), Actor::Human(human())).unwrap();

    let err = map
        .apply(
            add_edge(
                "resolves",
                node_ref("option", "Go"),
                node_ref("question", "Which language?"),
            ),
            Actor::Human(human()),
        )
        .err()
        .unwrap();

    assert_eq!(
        err,
        MapError::WrongEdgeEnd {
            edge_kind: "resolves".to_string(),
            end: EdgeEnd::From,
            allowed: vec!["decision".to_string()],
            found: "option".to_string(),
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
    let (question, option) = (NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", question, "question", "Which language?"),
        node_added("decisions", option, "option", "Go"),
        edge_added("decisions", "resolves", option, question),
    ];

    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    assert_eq!(map.edges().len(), 1);
}

#[test]
fn apply_removes_a_node_by_name_and_its_edges_with_it() {
    let mut map = Map::empty(decisions());
    map.apply(add_node("question", "Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("decision", "Rust over Go"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge(
            "resolves",
            node_ref("decision", "Rust over Go"),
            node_ref("question", "Which language?"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    let payload = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("question", "Which language?"),
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
    let mut map = Map::empty(decisions());
    map.apply(add_node("question", "Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(
        Mutation::AddNode {
            kind: "evidence".to_string(),
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
    map.apply(add_node("decision", "Rust over Go"), Actor::Human(human()))
        .unwrap();
    map.apply(
        add_edge(
            "resolves",
            node_ref("decision", "Rust over Go"),
            node_ref("question", "Which language?"),
        ),
        Actor::Human(human()),
    )
    .unwrap();

    assert_eq!(
        map.to_string(),
        "- question \"Which language?\"\n\
         - evidence \"Built both\": summary: \"side by\\nside\"; when: \"August\"\n\
         - decision \"Rust over Go\"\n\
         - decision \"Rust over Go\" resolves question \"Which language?\"\n"
    );
    assert_eq!(Map::empty(decisions()).to_string(), "");
}

#[test]
fn a_schema_is_found_by_name() {
    let schemas = crate::core::testing::schemas();
    assert_eq!(schemas.find("decisions").unwrap().name, "decisions");
    assert_eq!(
        schemas.find("glossary").err().unwrap().to_string(),
        "no map named \"glossary\"; maps are decisions, tasks"
    );
}

#[test]
fn every_kind_of_every_schema_carries_a_gloss() {
    for schema in [decisions(), tasks()] {
        for kind in &schema.node_kinds {
            assert!(
                !kind.gloss.is_empty(),
                "{}: kind {:?} has no gloss",
                schema.name,
                kind.name
            );
        }
        for kind in &schema.edge_kinds {
            assert!(
                !kind.gloss.is_empty(),
                "{}: kind {:?} has no gloss",
                schema.name,
                kind.name
            );
        }
    }
}

#[test]
fn a_question_requires_nothing() {
    assert!(decisions().node_kind("question").unwrap().requires.is_empty());
}

#[test]
fn an_undeclared_kind_is_absent() {
    assert!(decisions().node_kind("glossary").is_none());
}

#[test]
fn keeping_kinds_drops_other_nodes_and_the_edges_that_touched_them() {
    let (_, events) = rust_over_go();
    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    let cut = map.keep_kinds(&["decision".to_string()]).unwrap();
    assert_eq!(cut.nodes().len(), 1);
    assert!(cut.edges().is_empty());

    let both = map
        .keep_kinds(&["decision".to_string(), "question".to_string()])
        .unwrap();
    assert_eq!(both.nodes().len(), 2);
    assert_eq!(both.edges().len(), 1);
    assert_eq!(map.nodes().len(), 3, "the cut is a copy");
}

#[test]
fn keeping_a_kind_the_schema_lacks_is_an_error() {
    let map = Map::empty(decisions());
    let err = map.keep_kinds(&["goal".to_string()]).err().unwrap();
    assert!(matches!(err, MapError::UnknownNodeKind { .. }));
}

/// A chain: question <- decision, question <- option <- evidence, plus
/// an unlinked option, so depth walks one step at a time, against the
/// edge direction, and a genuinely unlinked node stays out at any
/// depth. `resolves`, `answers`, and `supports` are the only edge
/// kinds that can build it, since `Map::apply` now refuses an edge
/// whose ends are not of the kinds its edge kind declares.
fn chain() -> Map {
    let mut map = Map::empty(decisions());
    map.apply(add_node("question", "Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("decision", "Rust over Go"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("evidence", "Built both"), Actor::Human(human()))
        .unwrap();
    map.apply(add_option("Go"), Actor::Human(human())).unwrap();
    map.apply(add_option("Java"), Actor::Human(human())).unwrap();
    map.apply(
        add_edge(
            "resolves",
            node_ref("decision", "Rust over Go"),
            node_ref("question", "Which language?"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(
        add_edge(
            "answers",
            node_ref("option", "Go"),
            node_ref("question", "Which language?"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(
        add_edge(
            "supports",
            node_ref("evidence", "Built both"),
            node_ref("option", "Go"),
        ),
        Actor::Human(human()),
    )
    .unwrap();
    map
}

#[test]
fn around_at_depth_zero_is_the_node_alone() {
    let cut = chain()
        .around(&node_ref("decision", "Rust over Go"), 0)
        .unwrap();
    assert_eq!(cut.nodes().len(), 1);
    assert!(cut.edges().is_empty());
}

#[test]
fn around_follows_edges_both_ways_one_step_per_depth() {
    let map = chain();
    let one = map
        .around(&node_ref("question", "Which language?"), 1)
        .unwrap();
    let names: Vec<&str> = one.nodes().iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["Which language?", "Rust over Go", "Go"]);
    assert_eq!(one.edges().len(), 2);

    let two = map
        .around(&node_ref("question", "Which language?"), 2)
        .unwrap();
    assert_eq!(two.nodes().len(), 4, "the unlinked option stays out");
    assert_eq!(two.edges().len(), 3);
}

#[test]
fn select_counts_the_whole_and_the_edges_crossing_the_cut() {
    let whole = chain();
    let (nodes, edges) = (whole.nodes().len(), whole.edges().len());
    let question = node_ref("question", "Which language?");
    let selection = Selection {
        around: Some((&question, 1)),
        ..Selection::default()
    };

    let fragment = whole.select(&selection).unwrap();

    assert_eq!(fragment.map().nodes().len(), 3);
    assert_eq!(fragment.total_nodes(), nodes);
    assert_eq!(fragment.total_edges(), edges);
    assert_eq!(
        fragment.boundary_edges(),
        1,
        "the option's edge onward to evidence"
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
    let question = node_ref("question", "Which language?");
    let selection = Selection {
        around: Some((&question, 1)),
        ..Selection::default()
    };

    let fragment = Map::empty(decisions()).select(&selection).unwrap();

    assert_eq!(fragment.total_nodes(), 0);
    assert!(fragment.map().nodes().is_empty());
}

#[test]
fn since_on_a_log_folded_map_is_fine() {
    let selection = Selection {
        since: Some(Timestamp::now()),
        ..Selection::default()
    };

    assert!(Map::empty(decisions()).select(&selection).is_ok());
}

#[test]
fn select_walks_around_before_it_keeps_kinds() {
    let question = node_ref("question", "Which language?");
    let kinds = ["evidence".to_string()];
    let selection = Selection {
        around: Some((&question, 2)),
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
        "reached through the option, then kept alone"
    );
    assert_eq!(fragment.boundary_edges(), 1);
}

#[test]
fn around_a_node_the_map_lacks_is_an_error() {
    let err = chain()
        .around(&node_ref("option", "Rust"), 1)
        .err()
        .unwrap();
    assert!(matches!(
        &err,
        MapError::NoSuchNode { node, .. } if node == &node_ref("option", "Rust")
    ));
}

#[test]
fn a_missing_node_names_same_kind_nodes_that_share_a_word() {
    let err = chain()
        .around(&node_ref("decision", "Rust and Go"), 1)
        .err()
        .unwrap();
    assert_eq!(
        err.to_string(),
        "no decision \"Rust and Go\" in the map; did you mean decision:Rust over Go"
    );
}

#[test]
fn a_missing_node_stays_silent_on_a_single_shared_word() {
    let err = chain()
        .around(&node_ref("decision", "Rust and Java"), 1)
        .err()
        .unwrap();
    assert_eq!(err.to_string(), "no decision \"Rust and Java\" in the map");
}

#[test]
fn a_missing_node_counts_a_repeated_word_once() {
    let mut map = Map::empty(decisions());
    map.apply(add_node("decision", "safe safe pick"), Actor::Human(human()))
        .unwrap();
    let err = map
        .around(&node_ref("decision", "safe choice"), 1)
        .err()
        .unwrap();
    assert_eq!(err.to_string(), "no decision \"safe choice\" in the map");
}

#[test]
fn a_missing_node_names_a_matching_node_of_another_kind() {
    let err = chain()
        .around(&node_ref("question", "Rust over Go"), 1)
        .err()
        .unwrap();
    assert_eq!(
        err.to_string(),
        "no question \"Rust over Go\" in the map; did you mean decision:Rust over Go"
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
        .around(&node_ref("question", "Rust and Kotlin"), 1)
        .err()
        .unwrap();
    assert_eq!(
        err.to_string(),
        "no question \"Rust and Kotlin\" in the map"
    );
}

#[test]
fn a_short_id_is_its_kind_s_prefix_and_its_mint_order() {
    let mut map = Map::empty(decisions());
    map.apply(add_node("question", "Which language?"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("decision", "Rust"), Actor::Human(human()))
        .unwrap();
    map.apply(add_node("decision", "Go"), Actor::Human(human())).unwrap();

    let question = map.find("question", "Which language?").unwrap().id;
    let rust = map.find("decision", "Rust").unwrap().id;
    let go = map.find("decision", "Go").unwrap().id;

    assert_eq!(map.short_id(question), Some("q1".to_string()));
    assert_eq!(map.short_id(rust), Some("d1".to_string()));
    assert_eq!(map.short_id(go), Some("d2".to_string()));
}

/// `next_seq_by_kind` tracks a high-water mark per kind, never rolled
/// back on `NodeRemoved` - a short id is cited in chat, PR comments,
/// and committed text, so a removed node's number must never come back
/// under a different node.
#[test]
fn a_removed_node_s_number_is_never_reused() {
    let mut map = Map::empty(decisions());
    map.apply(add_node("decision", "Go"), Actor::Human(human())).unwrap();
    map.apply(
        Mutation::RemoveNode {
            node: node_ref("decision", "Go"),
            why: "reconsidered".to_string(),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
    map.apply(add_node("decision", "Rust"), Actor::Human(human()))
        .unwrap();

    let rust = map.find("decision", "Rust").unwrap().id;

    assert_eq!(map.short_id(rust), Some("d2".to_string()));
}

#[test]
fn resolve_str_takes_a_short_id_or_a_kind_and_name() {
    let mut map = Map::empty(decisions());
    map.apply(add_node("decision", "Rust over Go"), Actor::Human(human()))
        .unwrap();
    let id = map.find("decision", "Rust over Go").unwrap().id;

    assert_eq!(map.resolve_str("d1").unwrap(), id);
    assert_eq!(map.resolve_str("decision:Rust over Go").unwrap(), id);
}

#[test]
fn resolve_str_rejects_an_unknown_prefix_or_number() {
    let map = chain();

    assert!(matches!(
        map.resolve_str("z1"),
        Err(MapError::UnknownShortId(s)) if s == "z1"
    ));
    assert!(matches!(
        map.resolve_str("d99"),
        Err(MapError::UnknownShortId(s)) if s == "d99"
    ));
}

#[test]
fn a_node_added_event_with_no_seq_falls_back_to_its_position() {
    let events = [
        node_added("decisions", NodeId::new(), "decision", "A"),
        node_added("decisions", NodeId::new(), "decision", "B"),
    ];

    let map = Map::fold(decisions(), &scope(), &events).unwrap();

    let a = map.find("decision", "A").unwrap().id;
    let b = map.find("decision", "B").unwrap().id;
    assert_eq!(map.short_id(a), Some("d1".to_string()));
    assert_eq!(map.short_id(b), Some("d2".to_string()));
}
