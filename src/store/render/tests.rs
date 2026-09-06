use std::collections::BTreeMap;

use super::*;
use crate::percept::{Actor, EventId, Mutation, CODE, DECISIONS, SUPERSEDES};
use crate::testing::node_ref;

/// Adds a node with one `why` property when `why` is given.
fn add(map: &mut Map, kind: &str, name: &str, why: Option<&str>, sources: &[EventId], actor: Actor) {
    let properties = why
        .map(|why| BTreeMap::from([("why".to_string(), why.to_string())]))
        .unwrap_or_default();
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

fn link(map: &mut Map, kind: &str, from: (&str, &str), to: (&str, &str)) {
    map.apply(
        Mutation::AddEdge {
            kind: kind.to_string(),
            from: node_ref(from.0, from.1),
            to: node_ref(to.0, to.1),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();
}

fn heading(id: EventId) -> String {
    format!("{} \u{b7} {}", id.minted_at().unwrap().date(), id.as_uuid())
}

fn head() -> String {
    format!("# decisions\n\n{PREAMBLE} {DECISIONS_GUIDE}\n")
}

#[test]
fn an_empty_decisions_map_renders_the_preamble_and_the_empty_notice() {
    let text = markdown(&Map::empty(&DECISIONS));

    assert_eq!(
        text,
        "# decisions\n\
         \n\
         Folded from the percept log for this project and rerendered on every write. \
         Change it with `percept maps`, not by hand. Questions in the order they were \
         raised, grouped under the prompt that raised them, each with its decision. \
         Options and evidence: `percept maps show decisions --around 'question:<name>'`. \
         What changed lately: `percept maps show decisions --since 1d`.\n\
         \n\
         (empty: nothing has been recorded here yet.)\n"
    );
}

#[test]
fn questions_group_under_the_prompt_that_raised_them_in_first_seen_order() {
    let mut map = Map::empty(&DECISIONS);
    let (first, second) = (EventId::new(), EventId::new());
    add(&mut map, "question", "Where does the event log live?", None, &[first], Actor::User);
    add(
        &mut map,
        "decision",
        "one log under ~/.percept",
        Some("PERCEPT_HOME also holds the binary"),
        &[first],
        Actor::User,
    );
    link(
        &mut map,
        "resolves",
        ("decision", "one log under ~/.percept"),
        ("question", "Where does the event log live?"),
    );
    add(&mut map, "question", "How is a decision corrected?", None, &[second], Actor::User);
    add(
        &mut map,
        "decision",
        "add the new decision with a supersedes edge",
        Some("the old landmark stays one hop away"),
        &[second],
        Actor::User,
    );
    link(
        &mut map,
        "resolves",
        ("decision", "add the new decision with a supersedes edge"),
        ("question", "How is a decision corrected?"),
    );

    let expected = format!(
        "{}\n\
         ## {}\n\
         - \"Where does the event log live?\"\n\
         \x20 decision \"one log under ~/.percept\": why: \"PERCEPT_HOME also holds the binary\"\n\
         \n\
         ## {}\n\
         - \"How is a decision corrected?\"\n\
         \x20 decision \"add the new decision with a supersedes edge\": why: \"the old landmark stays one hop away\"\n",
        head(),
        heading(first),
        heading(second)
    );

    assert_eq!(markdown(&map), expected);
}

#[test]
fn a_question_without_a_decision_is_open() {
    let mut map = Map::empty(&DECISIONS);
    add(&mut map, "question", "Which key accepts a suggestion?", None, &[], Actor::User);

    assert_eq!(
        markdown(&map),
        format!(
            "{}\n\
             ## uncited\n\
             - \"Which key accepts a suggestion?\"\n\
             \x20 open\n",
            head()
        )
    );
}

#[test]
fn a_superseding_decision_settles_the_question_its_predecessor_resolved() {
    let mut map = Map::empty(&DECISIONS);
    let source = EventId::new();
    add(&mut map, "question", "Which model is default?", None, &[source], Actor::User);
    add(&mut map, "decision", "gpt3 by default", None, &[source], Actor::User);
    link(
        &mut map,
        "resolves",
        ("decision", "gpt3 by default"),
        ("question", "Which model is default?"),
    );
    add(&mut map, "decision", "gemma4 by default", None, &[source], Actor::User);
    link(
        &mut map,
        SUPERSEDES,
        ("decision", "gemma4 by default"),
        ("decision", "gpt3 by default"),
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}\n\
             ## {}\n\
             - \"Which model is default?\"\n\
             \x20 decision \"gemma4 by default\"\n\
             \x20 was \"gpt3 by default\"\n",
            head(),
            heading(source)
        )
    );
}

#[test]
fn a_supersession_chain_lists_every_predecessor_nearest_first() {
    let mut map = Map::empty(&DECISIONS);
    let source = EventId::new();
    add(&mut map, "question", "Which model?", None, &[source], Actor::User);
    for name in ["A", "B", "C"] {
        add(&mut map, "decision", name, None, &[source], Actor::User);
    }
    link(&mut map, "resolves", ("decision", "A"), ("question", "Which model?"));
    link(&mut map, SUPERSEDES, ("decision", "B"), ("decision", "A"));
    link(&mut map, SUPERSEDES, ("decision", "C"), ("decision", "B"));

    assert_eq!(
        markdown(&map),
        format!(
            "{}\n\
             ## {}\n\
             - \"Which model?\"\n\
             \x20 decision \"C\"\n\
             \x20 was \"B\"\n\
             \x20 was \"A\"\n",
            head(),
            heading(source)
        )
    );
}

#[test]
fn a_decision_citing_another_prompt_than_its_question_names_it() {
    let mut map = Map::empty(&DECISIONS);
    let (raised, settled) = (EventId::new(), EventId::new());
    add(&mut map, "question", "Which model?", None, &[raised], Actor::User);
    add(&mut map, "decision", "gemma4", None, &[settled], Actor::User);
    link(&mut map, "resolves", ("decision", "gemma4"), ("question", "Which model?"));

    assert_eq!(
        markdown(&map),
        format!(
            "{}\n\
             ## {}\n\
             - \"Which model?\"\n\
             \x20 decision \"gemma4\"\n\
             \x20 source {}\n",
            head(),
            heading(raised),
            settled.as_uuid()
        )
    );
}

#[test]
fn a_model_written_node_is_marked() {
    let mut map = Map::empty(&DECISIONS);
    let source = EventId::new();
    add(&mut map, "question", "Which key accepts a suggestion?", None, &[source], Actor::Model);

    assert_eq!(
        markdown(&map),
        format!(
            "{}\n\
             ## {}\n\
             - \"Which key accepts a suggestion?\" (model)\n\
             \x20 open\n",
            head(),
            heading(source)
        )
    );
}

#[test]
fn a_decision_resolving_no_question_is_its_own_bullet() {
    let mut map = Map::empty(&DECISIONS);
    let source = EventId::new();
    add(&mut map, "decision", "gemma4 by default", Some("the local model"), &[source], Actor::Model);

    assert_eq!(
        markdown(&map),
        format!(
            "{}\n\
             ## {}\n\
             - decision \"gemma4 by default\" (model): why: \"the local model\"\n",
            head(),
            heading(source)
        )
    );
}

#[test]
fn a_resolves_edge_between_the_wrong_kinds_settles_nothing() {
    let mut map = Map::empty(&DECISIONS);
    let source = EventId::new();
    add(&mut map, "option", "gemma4", None, &[source], Actor::User);
    add(&mut map, "decision", "gemma4 by default", None, &[source], Actor::User);
    link(&mut map, "resolves", ("decision", "gemma4 by default"), ("option", "gemma4"));

    assert!(markdown(&map).contains("- decision \"gemma4 by default\"\n"));
}

#[test]
fn a_map_with_no_question_or_decision_says_so() {
    let mut map = Map::empty(&DECISIONS);
    add(&mut map, "option", "Rust", None, &[EventId::new()], Actor::User);

    assert_eq!(
        markdown(&map),
        format!("{}\n(no question or decision yet; 1 nodes of other kinds.)\n", head())
    );
}

#[test]
fn a_map_of_another_schema_renders_per_kind() {
    let mut map = Map::empty(&CODE);
    add(&mut map, "file", "src/main.rs", None, &[], Actor::System);
    let cited = EventId::new();
    map.apply(
        Mutation::AddNode {
            kind: "function".to_string(),
            name: "main".to_string(),
            properties: BTreeMap::from([("returns".to_string(), "()".to_string())]),
            sources: vec![cited],
        },
        Actor::System,
    )
    .unwrap();
    link(&mut map, "contains", ("file", "src/main.rs"), ("function", "main"));

    let expected = format!(
        "# code\n\
         \n\
         Folded from the percept log for this project and rerendered on every write. \
         Change it with `percept maps`, not by hand.\n\
         \n\
         ## file\n\
         - \"src/main.rs\"\n\
         \n\
         ## function\n\
         - \"main\": returns: \"()\"\n\
         \x20 sources: {}\n\
         \n\
         ## edges\n\
         - file \"src/main.rs\" contains function \"main\"\n",
        cited.as_uuid()
    );

    assert_eq!(markdown(&map), expected);
}

#[test]
fn markdown_files_writes_the_map_named_file_in_its_directory_creating_it() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("maps");
    let renderer = MarkdownFiles::new(&dir);
    let map = Map::empty(&DECISIONS);

    renderer.render(&map).unwrap();

    let written = fs::read_to_string(dir.join("decisions.md")).unwrap();
    assert_eq!(written, markdown(&map));
}
