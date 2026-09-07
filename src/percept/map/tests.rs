use super::*;
use crate::percept::Actor;
use crate::testing::{node_added_at, scope, source, ROOT};

fn committed(payload: Payload) -> Event {
    Event::new(Actor::User, source("test"), None, payload)
}

#[test]
fn headlines_are_the_schema_s_headline_kinds_in_map_order() {
    let events = [
        node_added("decisions", NodeId::new(), "option", "Go"),
        node_added("decisions", NodeId::new(), "question", "Which language?"),
        node_added("decisions", NodeId::new(), "decision", "Rust"),
    ];
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();
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
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();
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
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

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
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

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
fn an_outcome_settles_a_task_the_way_a_decision_settles_a_question() {
    let (t, o, blocker) = (NodeId::new(), NodeId::new(), NodeId::new());
    let events = [
        node_added("tasks", t, "task", "cancel a turn"),
        node_added("tasks", blocker, "task", "cancellable streams"),
        node_added("tasks", o, "outcome", "done in 1a2b3c"),
        edge_added("tasks", RESOLVES, o, t),
        edge_added("tasks", "blocks", blocker, t),
    ];
    let map = Map::fold(&TASKS, &scope(), &events).unwrap();

    assert_eq!(
        map.settled_by(t).iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![o]
    );
    assert!(map.settles(o));
    assert_eq!(
        map.blocked_by(t).iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![blocker]
    );
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
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

    assert_eq!(
        map.weighed_for(q).iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![lost]
    );
}

#[test]
fn a_resolves_edge_between_other_kinds_settles_nothing() {
    let (o, d) = (NodeId::new(), NodeId::new());
    let events = [
        node_added("decisions", o, "option", "Go"),
        node_added("decisions", d, "decision", "Rust"),
        edge_added("decisions", RESOLVES, d, o),
    ];
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

    assert!(map.settled_by(o).is_empty());
    assert!(!map.settles(d));
}

/// `event`, re-stamped as created at `at`.
fn created_at(event: Event, at: Timestamp) -> Event {
    Event::restore(
        event.id(),
        event.actor(),
        event.source().clone(),
        event.causation_id(),
        at,
        event.payload().clone(),
    )
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
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

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
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

    let cut = map.since(at);

    let names: Vec<&str> = cut.nodes().iter().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["Rust"]);
    assert!(cut.edges().is_empty());
}

fn node_added(map: &str, node: NodeId, kind: &str, name: &str) -> Event {
    committed(Payload::NodeAdded {
        map: map.to_string(),
        node,
        kind: kind.to_string(),
        name: name.to_string(),
        properties: BTreeMap::new(),
        sources: Vec::new(),
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

fn node_removed(map: &str, node: NodeId) -> Event {
    committed(Payload::NodeRemoved {
        map: map.to_string(),
        node,
        reason: "gone".to_string(),
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

fn add_edge(kind: &str, from: NodeRef, to: NodeRef) -> Mutation {
    Mutation::AddEdge {
        kind: kind.to_string(),
        from,
        to,
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

#[test]
fn fold_stamps_a_node_with_its_events_actor_and_time() {
    let event = Event::new(
        Actor::Model,
        source("test"),
        None,
        Payload::NodeAdded {
            map: "decisions".to_string(),
            node: NodeId::new(),
            kind: "option".to_string(),
            name: "Rust".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
    );
    let created_at = event.created_at();

    let map = Map::fold(&DECISIONS, &scope(), &[event]).unwrap();

    let node = map.find("option", "Rust").unwrap();
    assert_eq!(node.actor, Actor::Model);
    assert_eq!(node.added_at, created_at);
}

#[test]
fn a_fold_holds_every_node_and_edge_still_present() {
    let (ids, events) = rust_over_go();

    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

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

    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
}

#[test]
fn a_fold_scoped_to_a_project_skips_a_node_added_under_another_path() {
    let events = [
        node_added_at(ROOT, "option", "Rust"),
        node_added_at("/other", "option", "Go"),
    ];

    let scoped = Map::fold(&DECISIONS, &scope(), &events).unwrap();
    assert_eq!(scoped.nodes().len(), 1);
    assert!(scoped.find("option", "Rust").is_some());

    let all = Map::fold(&DECISIONS, &Scope::All, &events).unwrap();
    assert_eq!(all.nodes().len(), 2);
}

#[test]
fn removing_a_node_drops_its_edges() {
    let (ids, mut events) = rust_over_go();
    events.push(node_removed("decisions", ids[0]));

    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

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
    }));

    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
    assert!(map.edges().is_empty());
}

#[test]
fn an_unknown_kind_fails_the_fold() {
    let stray = node_added("decisions", NodeId::new(), "goal", "Ship");
    let stray_id = stray.id();

    let err = Map::fold(&DECISIONS, &scope(), &[stray]).err().unwrap();

    assert_eq!(
        rejected_with(err, stray_id),
        MapError::UnknownNodeKind {
            map: &DECISIONS,
            kind: "goal".to_string()
        }
    );
    assert_eq!(
        MapError::UnknownNodeKind {
            map: &DECISIONS,
            kind: "goal".to_string()
        }
        .to_string(),
        "no node kind \"goal\" in map \"decisions\"; kinds are question, option, evidence, decision"
    );
}

#[test]
fn a_blank_name_fails_the_fold() {
    let stray = node_added("decisions", NodeId::new(), "option", " ");
    let stray_id = stray.id();

    let err = Map::fold(&DECISIONS, &scope(), &[stray]).err().unwrap();

    assert_eq!(rejected_with(err, stray_id), MapError::BlankName);
}

#[test]
fn a_name_is_unique_within_its_kind_only() {
    let events = vec![
        node_added("decisions", NodeId::new(), "option", "Rust"),
        node_added("decisions", NodeId::new(), "decision", "Rust"),
    ];
    assert_eq!(
        Map::fold(&DECISIONS, &scope(), &events)
            .unwrap()
            .nodes()
            .len(),
        2
    );

    let twice = node_added("decisions", NodeId::new(), "option", "Rust");
    let twice_id = twice.id();
    let mut events = events;
    events.push(twice);

    let err = Map::fold(&DECISIONS, &scope(), &events).err().unwrap();

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

    let err = Map::fold(&DECISIONS, &scope(), &with_dangling)
        .err()
        .unwrap();
    assert!(matches!(
        rejected_with(err, dangling_id),
        MapError::NoSuchNodeId(_)
    ));

    let twice = edge_added("decisions", "resolves", ids[2], ids[0]);
    let twice_id = twice.id();
    events.push(twice);

    let err = Map::fold(&DECISIONS, &scope(), &events).err().unwrap();
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
    });
    let stray_id = stray.id();
    events.push(stray);

    let err = Map::fold(&DECISIONS, &scope(), &events).err().unwrap();

    assert!(matches!(
        rejected_with(err, stray_id),
        MapError::NoSuchEdge { .. }
    ));
}

#[test]
fn apply_records_what_a_fold_rebuilds() {
    let mut built = Map::empty(&DECISIONS);
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
    .map(|m| committed(built.apply(m, Actor::User).unwrap()))
    .collect();

    let folded = Map::fold(&DECISIONS, &scope(), &events).unwrap();

    let decision = folded.find("decision", "Rust over Go").unwrap();
    assert!(decision.id == built.find("decision", "Rust over Go").unwrap().id);
    assert_eq!(folded.edges().len(), 1);
    assert!(folded.edges()[0].from == decision.id);
}

#[test]
fn apply_stamps_the_node_with_the_actor_given() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("option", "Rust"), Actor::User).unwrap();

    let node = map.find("option", "Rust").unwrap();

    assert_eq!(node.actor, Actor::User);
}

#[test]
fn apply_refuses_a_mutation_and_leaves_the_map_as_it_was() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("option", "Rust"), Actor::User).unwrap();

    let unknown = map
        .apply(add_node("goal", "Ship"), Actor::User)
        .err()
        .unwrap();
    let blank = map
        .apply(add_node("option", "  "), Actor::User)
        .err()
        .unwrap();
    let duplicate = map
        .apply(add_node("option", "Rust"), Actor::User)
        .err()
        .unwrap();
    let missing = map
        .apply(
            add_edge(
                "supports",
                node_ref("evidence", "Nope"),
                node_ref("option", "Rust"),
            ),
            Actor::User,
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
            },
            Actor::User,
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
fn apply_removes_a_node_by_name_and_its_edges_with_it() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("question", "Which language?"), Actor::User)
        .unwrap();
    map.apply(add_node("decision", "Rust over Go"), Actor::User)
        .unwrap();
    map.apply(
        add_edge(
            "resolves",
            node_ref("decision", "Rust over Go"),
            node_ref("question", "Which language?"),
        ),
        Actor::User,
    )
    .unwrap();

    let payload = map
        .apply(
            Mutation::RemoveNode {
                node: node_ref("question", "Which language?"),
                reason: "answered".to_string(),
                sources: Vec::new(),
            },
            Actor::User,
        )
        .unwrap();

    assert!(matches!(payload, Payload::NodeRemoved { .. }));
    assert_eq!(map.nodes().len(), 1);
    assert!(map.edges().is_empty());
}

#[test]
fn a_map_reads_as_one_line_per_node_then_per_edge() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("question", "Which language?"), Actor::User)
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
        Actor::User,
    )
    .unwrap();
    map.apply(add_node("decision", "Rust over Go"), Actor::User)
        .unwrap();
    map.apply(
        add_edge(
            "resolves",
            node_ref("decision", "Rust over Go"),
            node_ref("question", "Which language?"),
        ),
        Actor::User,
    )
    .unwrap();

    assert_eq!(
        map.to_string(),
        "- question \"Which language?\"\n\
         - evidence \"Built both\": summary: \"side by\\nside\"; when: \"August\"\n\
         - decision \"Rust over Go\"\n\
         - decision \"Rust over Go\" resolves question \"Which language?\"\n"
    );
    assert_eq!(Map::empty(&DECISIONS).to_string(), "");
}

#[test]
fn a_schema_is_found_by_name() {
    assert_eq!(Schema::find("decisions").unwrap().name, "decisions");
    assert_eq!(
        Schema::find("glossary").err().unwrap().to_string(),
        "no map named \"glossary\"; maps are decisions, tasks, code"
    );
}

#[test]
fn every_kind_of_every_schema_carries_a_gloss() {
    for schema in SCHEMAS.iter().chain(DERIVED) {
        for kind in schema.node_kinds.iter().chain(schema.edge_kinds) {
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
fn the_code_package_gloss_says_it_is_an_external_crate() {
    let package = CODE
        .node_kinds
        .iter()
        .find(|kind| kind.name == "package")
        .unwrap();
    assert!(package.gloss.contains("external crate"));
    assert!(package
        .gloss
        .contains("never one of this project's own modules"));
}

#[test]
fn keeping_kinds_drops_other_nodes_and_the_edges_that_touched_them() {
    let (_, events) = rust_over_go();
    let map = Map::fold(&DECISIONS, &scope(), &events).unwrap();

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
    let map = Map::empty(&DECISIONS);
    let err = map.keep_kinds(&["goal".to_string()]).err().unwrap();
    assert!(matches!(err, MapError::UnknownNodeKind { .. }));
}

/// A chain: question <- decision <- evidence, so depth walks one
/// step at a time and against the edge direction.
fn chain() -> Map {
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("question", "Which language?"), Actor::User)
        .unwrap();
    map.apply(add_node("decision", "Rust over Go"), Actor::User)
        .unwrap();
    map.apply(add_node("evidence", "Built both"), Actor::User)
        .unwrap();
    map.apply(add_node("option", "Go"), Actor::User).unwrap();
    map.apply(
        add_edge(
            "resolves",
            node_ref("decision", "Rust over Go"),
            node_ref("question", "Which language?"),
        ),
        Actor::User,
    )
    .unwrap();
    map.apply(
        add_edge(
            "supports",
            node_ref("evidence", "Built both"),
            node_ref("decision", "Rust over Go"),
        ),
        Actor::User,
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
    assert_eq!(names, ["Which language?", "Rust over Go"]);
    assert_eq!(one.edges().len(), 1);

    let two = map
        .around(&node_ref("question", "Which language?"), 2)
        .unwrap();
    assert_eq!(two.nodes().len(), 3, "the unlinked option stays out");
    assert_eq!(two.edges().len(), 2);
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

    assert_eq!(fragment.map().nodes().len(), 2);
    assert_eq!(fragment.total_nodes(), nodes);
    assert_eq!(fragment.total_edges(), edges);
    assert_eq!(fragment.boundary_edges(), 1, "the decision's edge onward");
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

    let fragment = Map::empty(&DECISIONS).select(&selection).unwrap();

    assert_eq!(fragment.total_nodes(), 0);
    assert!(fragment.map().nodes().is_empty());
}

#[test]
fn since_on_a_derived_map_is_refused_it_has_no_before() {
    let selection = Selection {
        since: Some(Timestamp::now()),
        ..Selection::default()
    };

    let err = Map::empty(&CODE).select(&selection).err().unwrap();

    assert_eq!(err, MapError::SinceOnDerived("code"));
}

#[test]
fn since_on_a_log_folded_map_is_fine() {
    let selection = Selection {
        since: Some(Timestamp::now()),
        ..Selection::default()
    };

    assert!(Map::empty(&DECISIONS).select(&selection).is_ok());
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
        "reached through the decision, then kept alone"
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
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("decision", "safe safe pick"), Actor::User)
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
    let mut map = Map::empty(&CODE);
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
fn a_derived_map_is_found_by_neither_fold_nor_write() {
    let err = Schema::find("code").err().unwrap();
    assert_eq!(err, MapError::Derived("code".to_string()));
    assert!(err
        .to_string()
        .starts_with("\"code\" is derived from the working tree"));
}
