use super::*;
use crate::percept::Actor;

fn committed(payload: Payload) -> Event {
    Event::new(Actor::User, "test".to_string(), None, payload)
}

#[test]
fn headlines_are_the_schema_s_headline_kinds_in_map_order() {
    let events = [
        node_added("decisions", NodeId::new(), "option", "Go"),
        node_added("decisions", NodeId::new(), "question", "Which language?"),
        node_added("decisions", NodeId::new(), "decision", "Rust"),
    ];
    let map = Map::fold(&DECISIONS, &events).unwrap();
    let names: Vec<&str> = map.headlines().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["Which language?", "Rust"]);
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
fn a_fold_holds_every_node_and_edge_still_present() {
    let (ids, events) = rust_over_go();

    let map = Map::fold(&DECISIONS, &events).unwrap();

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

    let map = Map::fold(&DECISIONS, &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
}

#[test]
fn removing_a_node_drops_its_edges() {
    let (ids, mut events) = rust_over_go();
    events.push(node_removed("decisions", ids[0]));

    let map = Map::fold(&DECISIONS, &events).unwrap();

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

    let map = Map::fold(&DECISIONS, &events).unwrap();

    assert_eq!(map.nodes().len(), 3);
    assert!(map.edges().is_empty());
}

#[test]
fn an_unknown_kind_fails_the_fold() {
    let stray = node_added("decisions", NodeId::new(), "goal", "Ship");
    let stray_id = stray.id();

    let err = Map::fold(&DECISIONS, &[stray]).err().unwrap();

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

    let err = Map::fold(&DECISIONS, &[stray]).err().unwrap();

    assert_eq!(rejected_with(err, stray_id), MapError::BlankName);
}

#[test]
fn a_name_is_unique_within_its_kind_only() {
    let events = vec![
        node_added("decisions", NodeId::new(), "option", "Rust"),
        node_added("decisions", NodeId::new(), "decision", "Rust"),
    ];
    assert_eq!(Map::fold(&DECISIONS, &events).unwrap().nodes().len(), 2);

    let twice = node_added("decisions", NodeId::new(), "option", "Rust");
    let twice_id = twice.id();
    let mut events = events;
    events.push(twice);

    let err = Map::fold(&DECISIONS, &events).err().unwrap();

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

    let err = Map::fold(&DECISIONS, &with_dangling).err().unwrap();
    assert!(matches!(
        rejected_with(err, dangling_id),
        MapError::NoSuchNodeId(_)
    ));

    let twice = edge_added("decisions", "resolves", ids[2], ids[0]);
    let twice_id = twice.id();
    events.push(twice);

    let err = Map::fold(&DECISIONS, &events).err().unwrap();
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

    let err = Map::fold(&DECISIONS, &events).err().unwrap();

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
    .map(|m| committed(built.apply(m).unwrap()))
    .collect();

    let folded = Map::fold(&DECISIONS, &events).unwrap();

    let decision = folded.find("decision", "Rust over Go").unwrap();
    assert!(decision.id == built.find("decision", "Rust over Go").unwrap().id);
    assert_eq!(folded.edges().len(), 1);
    assert!(folded.edges()[0].from == decision.id);
}

#[test]
fn apply_refuses_a_mutation_and_leaves_the_map_as_it_was() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("option", "Rust")).unwrap();

    let unknown = map.apply(add_node("goal", "Ship")).err().unwrap();
    let blank = map.apply(add_node("option", "  ")).err().unwrap();
    let duplicate = map.apply(add_node("option", "Rust")).err().unwrap();
    let missing = map
        .apply(add_edge(
            "supports",
            node_ref("evidence", "Nope"),
            node_ref("option", "Rust"),
        ))
        .err()
        .unwrap();
    let no_edge = map
        .apply(Mutation::RemoveEdge {
            kind: "supports".to_string(),
            from: node_ref("option", "Rust"),
            to: node_ref("option", "Rust"),
            sources: Vec::new(),
        })
        .err()
        .unwrap();

    assert!(matches!(unknown, MapError::UnknownNodeKind { .. }));
    assert_eq!(blank, MapError::BlankName);
    assert!(matches!(duplicate, MapError::DuplicateNode { .. }));
    assert_eq!(missing, MapError::NoSuchNode(node_ref("evidence", "Nope")));
    assert_eq!(missing.to_string(), "no evidence \"Nope\" in the map");
    assert!(matches!(no_edge, MapError::NoSuchEdge { .. }));
    assert_eq!(map.nodes().len(), 1);
    assert!(map.edges().is_empty());
}

#[test]
fn apply_removes_a_node_by_name_and_its_edges_with_it() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("question", "Which language?")).unwrap();
    map.apply(add_node("decision", "Rust over Go")).unwrap();
    map.apply(add_edge(
        "resolves",
        node_ref("decision", "Rust over Go"),
        node_ref("question", "Which language?"),
    ))
    .unwrap();

    let payload = map
        .apply(Mutation::RemoveNode {
            node: node_ref("question", "Which language?"),
            reason: "answered".to_string(),
            sources: Vec::new(),
        })
        .unwrap();

    assert!(matches!(payload, Payload::NodeRemoved { .. }));
    assert_eq!(map.nodes().len(), 1);
    assert!(map.edges().is_empty());
}

#[test]
fn a_map_reads_as_one_line_per_node_then_per_edge() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(add_node("question", "Which language?")).unwrap();
    map.apply(Mutation::AddNode {
        kind: "evidence".to_string(),
        name: "Built both".to_string(),
        properties: BTreeMap::from([
            ("summary".to_string(), "side by\nside".to_string()),
            ("when".to_string(), "August".to_string()),
        ]),
        sources: Vec::new(),
    })
    .unwrap();
    map.apply(add_node("decision", "Rust over Go")).unwrap();
    map.apply(add_edge(
        "resolves",
        node_ref("decision", "Rust over Go"),
        node_ref("question", "Which language?"),
    ))
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
        Schema::find("tasks").err().unwrap().to_string(),
        "no map named \"tasks\"; maps are decisions"
    );
}

#[test]
fn keeping_kinds_drops_other_nodes_and_the_edges_that_touched_them() {
    let (_, events) = rust_over_go();
    let map = Map::fold(&DECISIONS, &events).unwrap();

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
    map.apply(add_node("question", "Which language?")).unwrap();
    map.apply(add_node("decision", "Rust over Go")).unwrap();
    map.apply(add_node("evidence", "Built both")).unwrap();
    map.apply(add_node("option", "Go")).unwrap();
    map.apply(add_edge(
        "resolves",
        node_ref("decision", "Rust over Go"),
        node_ref("question", "Which language?"),
    ))
    .unwrap();
    map.apply(add_edge(
        "supports",
        node_ref("evidence", "Built both"),
        node_ref("decision", "Rust over Go"),
    ))
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
fn around_a_node_the_map_lacks_is_an_error() {
    let err = chain()
        .around(&node_ref("option", "Rust"), 1)
        .err()
        .unwrap();
    assert_eq!(err, MapError::NoSuchNode(node_ref("option", "Rust")));
}
