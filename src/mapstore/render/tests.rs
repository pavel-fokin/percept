use std::collections::BTreeMap;

use super::*;
use crate::core::testing::{decisions, files, human, node_ref, tasks};
use crate::core::{Actor, EventId, Mutation};

/// Adds a node with one `why` property when `why` is given, and one
/// `state` when `state` is given - `Map::apply` requires one for any
/// kind that declares states.
fn add(
    map: &mut Map,
    kind: &str,
    name: &str,
    why: Option<&str>,
    state: Option<&str>,
    sources: &[EventId],
    actor: Actor,
) {
    let mut properties = why
        .map(|why| BTreeMap::from([("why".to_string(), why.to_string())]))
        .unwrap_or_default();
    if let Some(state) = state {
        properties.insert("state".to_string(), state.to_string());
    }
    map.apply(
        Mutation::AddNode {
            kind: kind.to_string(),
            name: name.to_string(),
            properties,
            sources: sources.to_vec(),
        },
        actor,
    )
    .unwrap();
}

/// Changes a node already in `map`, merging `properties` into its own,
/// as `actor`, with `why` when the change carries one.
fn change(
    map: &mut Map,
    kind: &str,
    name: &str,
    properties: BTreeMap<String, String>,
    why: Option<&str>,
    actor: Actor,
) {
    map.apply(
        Mutation::ChangeNode {
            node: node_ref(kind, name),
            name: None,
            properties,
            sources: Vec::new(),
            why: why.map(str::to_string),
        },
        actor,
    )
    .unwrap();
}

fn link(map: &mut Map, kind: &str, from: (&str, &str), to: (&str, &str)) {
    map.apply(
        Mutation::AddEdge {
            kind: kind.to_string(),
            from: node_ref(from.0, from.1),
            to: node_ref(to.0, to.1),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
}

#[test]
fn an_empty_map_renders_its_title_and_the_empty_notice() {
    let map = Map::empty(decisions());
    assert_eq!(markdown(&map), "# decisions\n\n(empty: nothing has been recorded here yet.)\n");
}

#[test]
fn sections_order_headlines_by_state_then_by_when_they_were_added() {
    let mut map = Map::empty(tasks());
    add(&mut map, "task", "a", Some("first added"), Some("open"), &[], Actor::Human(human()));
    add(&mut map, "task", "b", Some("second added"), Some("done"), &[], Actor::Human(human()));
    add(&mut map, "task", "c", Some("third added"), Some("open"), &[], Actor::Human(human()));

    let text = markdown(&map);

    // "open" sorts before "done" - both before an added-order tiebreak
    // among tasks sharing a state; the core holds no such ordering, the
    // render alone gives this listing its meaning.
    let at = |name: &str| text.find(&format!("\n## {name}\n")).unwrap();
    assert!(at("t1 \"a\"") < at("t3 \"c\"") && at("t3 \"c\"") < at("t2 \"b\""), "{text}");
}

#[test]
fn an_option_is_printed_under_its_question_with_its_why() {
    let mut map = Map::empty(decisions());
    add(&mut map, "question", "Which parser?", None, None, &[], Actor::Human(human()));
    add(
        &mut map,
        "option",
        "reuse OpenAi",
        Some("the wire shapes differ"),
        None,
        &[],
        Actor::Human(human()),
    );
    link(&mut map, "answers", ("option", "reuse OpenAi"), ("question", "Which parser?"));

    let text = markdown(&map);

    assert!(
        text.contains(
            "## q1 \"Which parser?\"\n\
             \n\
             - o1 \"reuse OpenAi\" answers\n\
             \x20 why: \"the wire shapes differ\"\n"
        ),
        "{text}"
    );
}

#[test]
fn a_supersedes_and_a_reopens_edge_are_printed_by_name() {
    let mut map = Map::empty(decisions());
    add(&mut map, "decision", "Go", None, None, &[], Actor::Human(human()));
    add(&mut map, "decision", "Rust", None, None, &[], Actor::Human(human()));
    link(&mut map, "supersedes", ("decision", "Rust"), ("decision", "Go"));
    add(&mut map, "question", "Does Rust still fit?", None, None, &[], Actor::Human(human()));
    link(&mut map, "reopens", ("question", "Does Rust still fit?"), ("decision", "Rust"));

    let text = markdown(&map);

    assert!(
        text.contains("## d2 \"Rust\"\n\n- supersedes d1 \"Go\"\n- q1 \"Does Rust still fit?\" reopens\n"),
        "{text}"
    );
    assert!(text.contains("## d1 \"Go\"\n\n- d2 \"Rust\" supersedes\n"), "{text}");
}

#[test]
fn a_tasks_blocks_edge_is_printed_by_name_on_both_ends() {
    let mut map = Map::empty(tasks());
    add(&mut map, "task", "cancel a turn", Some("Esc drops the session"), Some("open"), &[], Actor::Human(human()));
    add(
        &mut map,
        "task",
        "cancellable streams",
        Some("nothing can stop a stream today"),
        Some("open"),
        &[],
        Actor::Human(human()),
    );
    link(&mut map, "blocks", ("task", "cancellable streams"), ("task", "cancel a turn"));

    let text = markdown(&map);

    assert!(
        text.contains("- t2 \"cancellable streams\" blocks\n"),
        "{text}"
    );
    assert!(
        text.contains("- blocks t1 \"cancel a turn\"\n"),
        "{text}"
    );
}

#[test]
fn a_map_with_no_headlines_says_so() {
    let map = Map::empty(decisions());
    // The map has a node, but no headline kind: no question or decision
    // was ever added.
    let mut map = map;
    add(&mut map, "option", "Rust", Some("only alternative weighed"), None, &[], Actor::Human(human()));

    assert_eq!(
        markdown(&map),
        "# decisions\n\n(no headline node yet; 1 nodes of other kinds.)\n"
    );
}

#[test]
fn a_model_written_node_is_marked() {
    let mut map = Map::empty(decisions());
    add(&mut map, "question", "Which key?", None, None, &[], Actor::Agent);

    let text = markdown(&map);

    assert!(text.contains("## q1 \"Which key?\" (agent)\n"), "{text}");
}

#[test]
fn a_changed_node_renders_its_changed_by_line_with_its_why() {
    let mut map = Map::empty(decisions());
    add(&mut map, "decision", "gemma4 by default", Some("the local model"), None, &[], Actor::Agent);
    change(
        &mut map,
        "decision",
        "gemma4 by default",
        BTreeMap::new(),
        Some("never proposed"),
        Actor::Human(human()),
    );

    let text = markdown(&map);

    assert!(text.contains("changed by human: \"never proposed\"\n"), "{text}");
}

#[test]
fn an_unchanged_node_carries_no_changed_by_line() {
    let mut map = Map::empty(decisions());
    add(&mut map, "decision", "gemma4 by default", None, None, &[], Actor::Human(human()));

    let text = markdown(&map);

    assert!(!text.contains("changed by"), "{text}");
}

#[test]
fn a_non_headline_neighbour_prints_its_own_properties_indented() {
    let mut map = Map::empty(files());
    add(&mut map, "file", "src/main.rs", None, None, &[], Actor::System);
    map.apply(
        Mutation::AddNode {
            kind: "function".to_string(),
            name: "main".to_string(),
            properties: BTreeMap::from([("returns".to_string(), "()".to_string())]),
            sources: Vec::new(),
        },
        Actor::System,
    )
    .unwrap();
    link(&mut map, "contains", ("file", "src/main.rs"), ("function", "main"));

    let text = markdown(&map);

    assert!(
        text.contains(
            "## f1 \"src/main.rs\"\n\
             \n\
             - contains fn1 \"main\"\n\
             \x20 returns: \"()\"\n"
        ),
        "{text}"
    );
}

/// A schema with no headline kind at all falls back to `push_by_kind`:
/// one `## <kind>` section per kind that holds a node, then `## edges`.
#[test]
fn a_schema_with_no_headline_kind_falls_back_to_a_section_per_kind() {
    let schema = crate::core::Schema {
        name: "glossary".to_string(),
        purpose: "test fixture".to_string(),
        node_kinds: vec![crate::core::NodeKind::new("term", "a word")],
        edge_kinds: Vec::new(),
        headline_kinds: Vec::new(),
    };
    let mut map = Map::empty(schema);
    add(&mut map, "term", "harness", None, None, &[], Actor::Human(human()));

    let text = markdown(&map);

    assert!(text.contains("\n## term\n- t1 \"harness\"\n"), "{text}");
}

#[test]
fn the_catalogue_gives_each_map_a_section_listing_its_kinds() {
    let mut map = Map::empty(decisions());
    add(&mut map, "question", "Where does the log live?", None, None, &[], Actor::Human(human()));

    let text = catalogue(std::slice::from_ref(&map));

    assert!(text.starts_with("# maps\n"));
    assert!(text.contains("## decisions\n"));
    assert!(text.contains(&decisions().purpose));
    assert!(text.contains("1 nodes, 0 edges.\n"));
    assert!(text.contains(
        "- `contradicts` (evidence -> option)\n- `resolves` (decision -> question)"
    ));
    assert!(text.contains("\nExample node and edge:\n\n    {\"node\":"));
    assert!(text.contains("\"name\":\"Where does the log live?\""));
}

#[test]
fn a_kind_with_no_gloss_renders_without_a_trailing_dash() {
    let text = catalogue(&[Map::empty(decisions())]);

    assert!(
        text.contains("\nNode kinds:\n- `question`\n"),
        "{text}"
    );
}

#[test]
fn the_catalogue_names_a_kinds_required_properties() {
    let text = catalogue(&[Map::empty(decisions())]);

    assert!(
        text.contains(
            "- `option` (requires `why`) - an alternative that was weighed and lost, saying \
             why in its `why` property"
        ),
        "{text}"
    );
}

#[test]
fn the_catalogue_glosses_a_package_kind_as_an_external_crate() {
    let text = catalogue(&[Map::empty(files())]);

    assert!(text.contains("- `package` - an external crate a file imports"));
    assert!(text.contains("never one of this project's own modules"));
    assert!(text.contains("\nExample: nothing recorded here yet.\n"));
}
