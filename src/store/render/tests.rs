use std::collections::BTreeMap;

use super::*;
use crate::percept::{Actor, EventId, Mutation, CODE, DECISIONS, SUPERSEDES};
use crate::testing::node_ref;

#[test]
fn an_empty_decisions_map_renders_the_preamble_and_the_empty_notice() {
    let text = markdown(&Map::empty(&DECISIONS));

    assert_eq!(
        text,
        "# decisions\n\
         \n\
         Folded from the percept log for this project and rerendered on every write. \
         Change it with `percept maps`, not by hand. Questions in the order they were \
         raised, grouped under the prompt that settled them, each with its decision. \
         Options and evidence: `percept maps show decisions --around 'question:<name>'`. \
         What changed lately: `percept maps show decisions --since 1d`.\n\
         \n\
         (empty: nothing has been recorded here yet.)\n"
    );
}

#[test]
fn questions_group_under_the_prompt_that_settled_them_in_first_seen_order() {
    let mut map = Map::empty(&DECISIONS);
    let first = EventId::new();
    let second = EventId::new();

    map.apply(
        Mutation::AddNode {
            kind: "question".to_string(),
            name: "Where does the event log live?".to_string(),
            properties: BTreeMap::new(),
            sources: vec![first],
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddNode {
            kind: "decision".to_string(),
            name: "one log under ~/.percept".to_string(),
            properties: BTreeMap::from([(
                "why".to_string(),
                "PERCEPT_HOME also holds the binary".to_string(),
            )]),
            sources: vec![first],
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddEdge {
            kind: "resolves".to_string(),
            from: node_ref("decision", "one log under ~/.percept"),
            to: node_ref("question", "Where does the event log live?"),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();

    map.apply(
        Mutation::AddNode {
            kind: "question".to_string(),
            name: "How is a decision corrected?".to_string(),
            properties: BTreeMap::new(),
            sources: vec![second],
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddNode {
            kind: "decision".to_string(),
            name: "add the new decision with a supersedes edge".to_string(),
            properties: BTreeMap::from([(
                "why".to_string(),
                "the old landmark stays one hop away".to_string(),
            )]),
            sources: vec![second],
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddEdge {
            kind: "resolves".to_string(),
            from: node_ref("decision", "add the new decision with a supersedes edge"),
            to: node_ref("question", "How is a decision corrected?"),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();

    let first_heading = format!(
        "{} \u{b7} {}",
        first.minted_at().unwrap().date(),
        first.as_uuid()
    );
    let second_heading = format!(
        "{} \u{b7} {}",
        second.minted_at().unwrap().date(),
        second.as_uuid()
    );

    let expected = format!(
        "# decisions\n\
         \n\
         Folded from the percept log for this project and rerendered on every write. \
         Change it with `percept maps`, not by hand. Questions in the order they were \
         raised, grouped under the prompt that settled them, each with its decision. \
         Options and evidence: `percept maps show decisions --around 'question:<name>'`. \
         What changed lately: `percept maps show decisions --since 1d`.\n\
         \n\
         ## {first_heading}\n\
         - \"Where does the event log live?\"\n\
         \x20 decision \"one log under ~/.percept\": why: \"PERCEPT_HOME also holds the binary\"\n\
         \n\
         ## {second_heading}\n\
         - \"How is a decision corrected?\"\n\
         \x20 decision \"add the new decision with a supersedes edge\": why: \"the old landmark stays one hop away\"\n"
    );

    assert_eq!(markdown(&map), expected);
}

#[test]
fn a_question_without_a_decision_is_open() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(
        Mutation::AddNode {
            kind: "question".to_string(),
            name: "Which key accepts a suggestion?".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();

    assert_eq!(
        markdown(&map),
        format!(
            "# decisions\n\
             \n\
             {DECISIONS_PREAMBLE}\n\
             \n\
             ## uncited\n\
             - \"Which key accepts a suggestion?\"\n\
             \x20 open\n"
        )
    );
}

#[test]
fn a_superseded_decision_renders_as_was_under_its_successor() {
    let mut map = Map::empty(&DECISIONS);
    let source = EventId::new();
    map.apply(
        Mutation::AddNode {
            kind: "question".to_string(),
            name: "Which model is default?".to_string(),
            properties: BTreeMap::new(),
            sources: vec![source],
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddNode {
            kind: "decision".to_string(),
            name: "gpt3 by default".to_string(),
            properties: BTreeMap::new(),
            sources: vec![source],
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddEdge {
            kind: "resolves".to_string(),
            from: node_ref("decision", "gpt3 by default"),
            to: node_ref("question", "Which model is default?"),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddNode {
            kind: "decision".to_string(),
            name: "gemma4 by default".to_string(),
            properties: BTreeMap::new(),
            sources: vec![source],
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddEdge {
            kind: "resolves".to_string(),
            from: node_ref("decision", "gemma4 by default"),
            to: node_ref("question", "Which model is default?"),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();
    map.apply(
        Mutation::AddEdge {
            kind: SUPERSEDES.to_string(),
            from: node_ref("decision", "gemma4 by default"),
            to: node_ref("decision", "gpt3 by default"),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();

    let heading = format!(
        "{} \u{b7} {}",
        source.minted_at().unwrap().date(),
        source.as_uuid()
    );

    assert_eq!(
        markdown(&map),
        format!(
            "# decisions\n\
             \n\
             {DECISIONS_PREAMBLE}\n\
             \n\
             ## {heading}\n\
             - \"Which model is default?\"\n\
             \x20 decision \"gemma4 by default\"\n\
             \x20 was \"gpt3 by default\"\n"
        )
    );
}

#[test]
fn a_model_written_node_is_marked() {
    let mut map = Map::empty(&DECISIONS);
    let source = EventId::new();
    map.apply(
        Mutation::AddNode {
            kind: "question".to_string(),
            name: "Which key accepts a suggestion?".to_string(),
            properties: BTreeMap::new(),
            sources: vec![source],
        },
        Actor::Model,
    )
    .unwrap();

    let heading = format!(
        "{} \u{b7} {}",
        source.minted_at().unwrap().date(),
        source.as_uuid()
    );

    assert_eq!(
        markdown(&map),
        format!(
            "# decisions\n\
             \n\
             {DECISIONS_PREAMBLE}\n\
             \n\
             ## {heading}\n\
             - \"Which key accepts a suggestion?\" (model)\n\
             \x20 open\n"
        )
    );
}

#[test]
fn an_uncited_question_goes_under_uncited() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(
        Mutation::AddNode {
            kind: "question".to_string(),
            name: "Which key accepts a suggestion?".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();

    assert!(markdown(&map).contains("\n## uncited\n"));
}

#[test]
fn a_decision_resolving_no_question_is_its_own_bullet() {
    let mut map = Map::empty(&DECISIONS);
    let source = EventId::new();
    map.apply(
        Mutation::AddNode {
            kind: "decision".to_string(),
            name: "gemma4 by default".to_string(),
            properties: BTreeMap::from([("why".to_string(), "the local model".to_string())]),
            sources: vec![source],
        },
        Actor::Model,
    )
    .unwrap();

    let heading = format!(
        "{} \u{b7} {}",
        source.minted_at().unwrap().date(),
        source.as_uuid()
    );

    assert_eq!(
        markdown(&map),
        format!(
            "# decisions\n\
             \n\
             {DECISIONS_PREAMBLE}\n\
             \n\
             ## {heading}\n\
             - decision \"gemma4 by default\" (model): why: \"the local model\"\n"
        )
    );
}

#[test]
fn a_map_of_another_schema_renders_per_kind() {
    let mut map = Map::empty(&CODE);
    map.apply(
        Mutation::AddNode {
            kind: "file".to_string(),
            name: "src/main.rs".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
        Actor::System,
    )
    .unwrap();
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
    map.apply(
        Mutation::AddEdge {
            kind: "contains".to_string(),
            from: node_ref("file", "src/main.rs"),
            to: node_ref("function", "main"),
            sources: Vec::new(),
        },
        Actor::System,
    )
    .unwrap();

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
