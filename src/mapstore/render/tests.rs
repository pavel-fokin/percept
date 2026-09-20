use std::collections::BTreeMap;

use super::*;
use crate::core::testing::{chores, debates, files, human, link, node_ref};
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
/// as `actor`.
fn change(map: &mut Map, kind: &str, name: &str, properties: BTreeMap<String, String>, actor: Actor) {
    map.apply(
        Mutation::ChangeNode {
            node: node_ref(kind, name),
            name: None,
            properties,
            sources: Vec::new(),
        },
        actor,
    )
    .unwrap();
}


#[test]
fn an_empty_map_renders_its_title_and_the_empty_notice() {
    let map = Map::empty(crate::core::testing::map_id("debates"), debates());
    assert_eq!(markdown(&map), "# debates\n\n(empty: nothing has been recorded here yet.)\n");
}

#[test]
fn sections_are_ordered_by_state_then_by_when_they_were_added() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    add(&mut map, "chore", "a", Some("first added"), Some("open"), &[], Actor::Human(human()));
    add(&mut map, "chore", "b", Some("second added"), Some("done"), &[], Actor::Human(human()));
    add(&mut map, "chore", "c", Some("third added"), Some("open"), &[], Actor::Human(human()));

    let text = markdown(&map);

    // "open" sorts before "done" - both before an added-order tiebreak
    // among chores sharing a state; the core holds no such ordering, the
    // render alone gives this listing its meaning.
    let at = |name: &str| text.find(&format!("\n## {name}\n")).unwrap();
    assert!(at("c1 \"a\"") < at("c3 \"c\"") && at("c3 \"c\"") < at("c2 \"b\""), "{text}");
}

#[test]
fn sections_with_no_state_are_ordered_by_when_they_were_added() {
    // `push_sections` now sorts roots only by `state_rank` then
    // `added_at` - see its doc comment, which still says kind order
    // breaks the tie first; it no longer does, so two roots of
    // different kinds and no state sort by add order alone.
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "verdict", "ship it", None, None, &[], Actor::Human(human()));
    add(&mut map, "topic", "Which parser?", None, None, &[], Actor::Human(human()));

    let text = markdown(&map);

    let at = |name: &str| text.find(&format!("\n## {name}\n")).unwrap();
    assert!(at("v1 \"ship it\"") < at("t1 \"Which parser?\""), "{text}");
}

#[test]
fn a_claim_is_printed_under_its_topic_with_its_why() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "topic", "Which parser?", None, None, &[], Actor::Human(human()));
    add(
        &mut map,
        "claim",
        "reuse OpenAi",
        Some("the wire shapes differ"),
        None,
        &[],
        Actor::Human(human()),
    );
    link(&mut map, "about", ("topic", "Which parser?"), ("claim", "reuse OpenAi"));

    let text = markdown(&map);

    assert!(
        text.contains(
            "## t1 \"Which parser?\"\n\
             \n\
             - about c1 \"reuse OpenAi\"\n\
             \x20\x20why: \"the wire shapes differ\"\n"
        ),
        "{text}"
    );
}

#[test]
fn a_replaced_verdict_and_a_doubting_topic_nest_under_the_verdict_that_claims_them() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "verdict", "Go", None, None, &[], Actor::Human(human()));
    add(&mut map, "verdict", "Rust", None, None, &[], Actor::Human(human()));
    link(&mut map, "replaces", ("verdict", "Rust"), ("verdict", "Go"));
    add(&mut map, "topic", "Does Rust still fit?", None, None, &[], Actor::Human(human()));
    link(&mut map, "doubts", ("verdict", "Rust"), ("topic", "Does Rust still fit?"));

    let text = markdown(&map);

    assert_eq!(
        text,
        "# debates\n\
         \n\
         ## v2 \"Rust\"\n\
         \n\
         - replaces v1 \"Go\"\n\
         - doubts t1 \"Does Rust still fit?\"\n"
    );
}

#[test]
fn a_verdict_nests_under_its_topic_and_the_one_it_replaces_under_it() {
    // A forest allows only one parent per node, so `Go`, once
    // superseded, hangs under `Rust` alone - through `replaces` - and
    // is not also settled directly under the topic.
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "topic", "Which language?", None, None, &[], Actor::Human(human()));
    add(&mut map, "verdict", "Go", None, None, &[], Actor::Human(human()));
    add(&mut map, "verdict", "Rust", None, None, &[], Actor::Human(human()));
    link(&mut map, "settles", ("topic", "Which language?"), ("verdict", "Rust"));
    link(&mut map, "replaces", ("verdict", "Rust"), ("verdict", "Go"));
    add(&mut map, "claim", "Java", Some("no one here writes it"), None, &[], Actor::Human(human()));
    link(&mut map, "about", ("topic", "Which language?"), ("claim", "Java"));

    let text = markdown(&map);

    assert_eq!(
        text,
        "# debates\n\
         \n\
         ## t1 \"Which language?\"\n\
         \n\
         - settles v2 \"Rust\"\n\
         \x20\x20- replaces v1 \"Go\"\n\
         - about c1 \"Java\"\n\
         \x20\x20why: \"no one here writes it\"\n"
    );
}

#[test]
fn a_blocked_chore_nests_under_the_chore_that_blocks_it() {
    let mut map = Map::empty(crate::core::testing::map_id("chores"), chores());
    add(&mut map, "chore", "cancel a turn", Some("Esc drops the session"), Some("open"), &[], Actor::Human(human()));
    add(
        &mut map,
        "chore",
        "cancellable streams",
        Some("nothing can stop a stream today"),
        Some("open"),
        &[],
        Actor::Human(human()),
    );
    link(&mut map, "blocks", ("chore", "cancellable streams"), ("chore", "cancel a turn"));

    let text = markdown(&map);

    assert!(
        text.contains("## c2 \"cancellable streams\"\n"),
        "{text}"
    );
    assert!(
        text.contains("- blocks c1 \"cancel a turn\"\n"),
        "{text}"
    );
    assert!(!text.contains("## c1"), "{text}");
}

#[test]
fn a_claim_pointed_at_by_no_edge_heads_its_own_section() {
    // `claim` is a section kind - `backs` names it as a `to` end - so a
    // claim nobody points at now heads a section of its own, where
    // before only `topic` and `verdict` could.
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "claim", "Rust", Some("only alternative weighed"), None, &[], Actor::Human(human()));

    assert_eq!(
        markdown(&map),
        "# debates\n\n## c1 \"Rust\"\n\nwhy: \"only alternative weighed\"\n"
    );
}

#[test]
fn a_map_the_agent_wrote_alone_is_marked_once_on_its_heading() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "topic", "Which key?", None, None, &[], Actor::Agent);

    let text = markdown(&map);

    assert!(text.starts_with("# debates (agent)\n"), "{text}");
    assert!(text.contains("## t1 \"Which key?\"\n"), "{text}");
}

#[test]
fn a_map_with_both_authors_marks_each_agent_node() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "topic", "Which key?", None, None, &[], Actor::Agent);
    add(&mut map, "topic", "Which lock?", None, None, &[], Actor::Human(human()));

    let text = markdown(&map);

    assert!(text.starts_with("# debates\n"), "{text}");
    assert!(text.contains("## t1 \"Which key?\" (agent)\n"), "{text}");
    assert!(text.contains("## t2 \"Which lock?\"\n"), "{text}");
}

#[test]
fn a_changed_node_renders_its_changed_by_line() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "verdict", "gemma4 by default", Some("the local model"), None, &[], Actor::Agent);
    change(
        &mut map,
        "verdict",
        "gemma4 by default",
        BTreeMap::from([("why".to_string(), "never proposed".to_string())]),
        Actor::Human(human()),
    );

    let text = markdown(&map);

    assert!(text.contains("changed by human\n"), "{text}");
}

#[test]
fn an_unchanged_node_carries_no_changed_by_line() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "verdict", "gemma4 by default", None, None, &[], Actor::Human(human()));

    let text = markdown(&map);

    assert!(!text.contains("changed by"), "{text}");
}

#[test]
fn a_file_heads_its_section_and_the_symbols_it_contains_nest_under_it() {
    // `contains` runs from a file to a symbol it defines, so the file
    // heads the section - it has no parent - and the function it
    // contains nests under it.
    let mut map = Map::empty(crate::core::testing::map_id("files"), files());
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

    assert_eq!(
        text,
        "# files\n\
         \n\
         ## f1 \"src/main.rs\"\n\
         \n\
         - contains fn1 \"main\"\n\
         \x20\x20returns: \"()\"\n"
    );
}

/// A forest, so a node hangs under exactly one section and is named
/// only there.
#[test]
fn a_node_is_named_under_the_one_section_it_hangs_in() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "topic", "Which parser?", None, None, &[], Actor::Human(human()));
    add(&mut map, "verdict", "ship it", None, None, &[], Actor::Human(human()));
    link(&mut map, "settles", ("topic", "Which parser?"), ("verdict", "ship it"));

    let text = markdown(&map);

    assert!(text.contains("## t1 \"Which parser?\"\n\n- settles v1 \"ship it\"\n"), "{text}");
    assert!(!text.contains("\n## v1 "), "{text}");
}

/// A schema declaring no edge at all: nothing can reach a node, so
/// every node heads a section of its own.
#[test]
fn a_schema_with_no_edges_gives_every_node_its_own_section() {
    let schema = crate::core::Schema {
        name: "glossary".to_string(),
        purpose: "test fixture".to_string(),
        node_kinds: vec![crate::core::NodeKind::new("term")],
        edge_kinds: Vec::new(),
        rules: crate::core::Rules::default(),
    };
    let mut map = Map::empty(crate::core::MapId::new(), schema);
    add(&mut map, "term", "harness", None, None, &[], Actor::Human(human()));

    // Nothing can reach a node in a schema that declares no edge, so
    // every node heads a section - the shape a first schema takes
    // before its edges are written.
    assert_eq!(markdown(&map), "# glossary\n\n## t1 \"harness\"\n\n");
}

#[test]
fn the_catalogue_gives_each_map_a_section_listing_its_kinds() {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), debates());
    add(&mut map, "topic", "Where does the log live?", None, None, &[], Actor::Human(human()));

    let text = catalogue(std::slice::from_ref(&map));

    assert!(text.starts_with("# maps\n"));
    assert!(text.contains("## debates\n"));
    assert!(text.contains(&debates().purpose));
    assert!(text.contains("1 nodes, 0 edges.\n"));
    assert!(text.contains(
        "- `backs` (claim -> fact)\n- `settles` (topic -> verdict)"
    ));
    assert!(text.contains("\nExample node and edge:\n\n    {\"node\":"));
    assert!(text.contains("\"name\":\"Where does the log live?\""));
}

#[test]
fn the_catalogue_lists_a_kinds_one_carried_property() {
    let text = catalogue(&[Map::empty(crate::core::testing::map_id("debates"), debates())]);

    assert!(text.contains("\n- `verdict` (carries `why`)\n"), "{text}");
}

#[test]
fn the_catalogue_names_a_kinds_carried_properties() {
    let text = catalogue(&[Map::empty(crate::core::testing::map_id("debates"), debates())]);

    assert!(text.contains("- `claim` (carries `why`, `summary`)"), "{text}");
}

#[test]
fn the_catalogue_of_no_maps_prints_the_no_schemas_hint() {
    let text = catalogue(&[]);

    assert_eq!(text, format!("# maps\n\n{}\n", crate::mapstore::NO_SCHEMAS_HINT));
}

#[test]
fn the_catalogue_lists_a_package_kind_with_no_properties() {
    let text = catalogue(&[Map::empty(crate::core::testing::map_id("files"), files())]);

    assert!(text.contains("- `package`\n"));
    assert!(text.contains("\nExample: nothing recorded here yet.\n"));
}
