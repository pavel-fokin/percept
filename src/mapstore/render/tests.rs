use std::collections::BTreeMap;

use super::*;
use crate::core::testing::{decisions, edge_added, files, human, node_added_citing, node_ref, tasks};
use crate::core::{Actor, EventId, Mutation, REOPENS, SUPERSEDES};

/// Adds a node with one `why` property when `why` is given. A `task`
/// also needs a `state` on add now that `Map::apply` requires one for
/// any kind that declares states; `open` is the one every fresh task
/// in these fixtures starts at.
fn add(
    map: &mut Map,
    kind: &str,
    name: &str,
    why: Option<&str>,
    sources: &[EventId],
    actor: Actor,
) {
    let mut properties = why
        .map(|why| BTreeMap::from([("why".to_string(), why.to_string())]))
        .unwrap_or_default();
    if kind == "task" {
        properties.insert("state".to_string(), "open".to_string());
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

/// Changes a node already in `map`, merging `properties` into its own.
fn change(
    map: &mut Map,
    kind: &str,
    name: &str,
    properties: BTreeMap<String, String>,
    actor: Actor,
) {
    map.apply(
        Mutation::ChangeNode {
            node: node_ref(kind, name),
            name: None,
            properties,
            sources: Vec::new(),
            why: None,
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

fn head() -> String {
    format!("# decisions\n\n{PREAMBLE} {DECISIONS_GUIDE}\n")
}

fn tasks_head() -> String {
    format!("# tasks\n\n{PREAMBLE} {TASKS_GUIDE}\n")
}

/// A `## contents` block: one `- id "name" · date` line per entry,
/// `id` the short id its kind's prefix and mint order give it.
fn contents(entries: &[(&str, &str, EventId)]) -> String {
    let mut out = "\n## contents\n".to_string();
    for (id, name, source) in entries {
        out.push_str(&format!(
            "- {id} {name:?} \u{b7} {}\n",
            source.minted_at().unwrap().date()
        ));
    }
    out
}

#[test]
fn an_empty_decisions_map_renders_the_preamble_and_the_empty_notice() {
    assert_eq!(
        markdown(&Map::empty(decisions())),
        format!("{}\n(empty: nothing has been recorded here yet.)\n", head())
    );
}

#[test]
fn questions_render_flat_at_h2_in_first_seen_order() {
    let mut map = Map::empty(decisions());
    let (first, second) = (EventId::new(), EventId::new());
    add(
        &mut map,
        "question",
        "Where does the event log live?",
        None,
        &[first],
        Actor::Human(human()),
    );
    add(
        &mut map,
        "decision",
        "one log under ~/.percept",
        Some("PERCEPT_HOME also holds the binary"),
        &[first],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "resolves",
        ("decision", "one log under ~/.percept"),
        ("question", "Where does the event log live?"),
    );
    add(
        &mut map,
        "question",
        "How is a decision corrected?",
        None,
        &[second],
        Actor::Human(human()),
    );
    add(
        &mut map,
        "decision",
        "add the new decision with a supersedes edge",
        Some("the old landmark stays one hop away"),
        &[second],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "resolves",
        ("decision", "add the new decision with a supersedes edge"),
        ("question", "How is a decision corrected?"),
    );

    let expected = format!(
        "{}{}\n\
         ## q1 \"Where does the event log live?\"\n\
         \n\
         - decision d1 \"one log under ~/.percept\"\n\
         \x20 why: \"PERCEPT_HOME also holds the binary\"\n\
         \n\
         ## q2 \"How is a decision corrected?\"\n\
         \n\
         - decision d2 \"add the new decision with a supersedes edge\"\n\
         \x20 why: \"the old landmark stays one hop away\"\n",
        head(),
        contents(&[
            ("q1", "Where does the event log live?", first),
            ("q2", "How is a decision corrected?", second),
        ]),
    );

    assert_eq!(markdown(&map), expected);
}

#[test]
fn the_contents_list_names_every_question_with_its_raising_date() {
    let mut map = Map::empty(decisions());
    let prompt = EventId::new();
    for name in ["Which model?", "Where is the key?", "What is the URL?"] {
        add(&mut map, "question", name, None, &[prompt], Actor::Human(human()));
    }

    let text = markdown(&map);

    assert!(text.contains(&format!(
        "\n## contents\n\
         - q1 \"Which model?\" \u{b7} {date}\n\
         - q2 \"Where is the key?\" \u{b7} {date}\n\
         - q3 \"What is the URL?\" \u{b7} {date}\n\
         \n## q1 \"Which model?\"\n",
        date = prompt.minted_at().unwrap().date()
    )));
}

#[test]
fn a_questions_own_properties_render_under_its_heading() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(
        &mut map,
        "question",
        "How should X integrate?",
        Some("not decided; three models sketched"),
        &[source],
        Actor::Human(human()),
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}{}\n\
             ## q1 \"How should X integrate?\"\n\
             \n\
             why: \"not decided; three models sketched\"\n\
             - open\n",
            head(),
            contents(&[("q1", "How should X integrate?", source)]),
        )
    );
}

#[test]
fn a_question_without_a_decision_is_open() {
    let mut map = Map::empty(decisions());
    add(
        &mut map,
        "question",
        "Which key accepts a suggestion?",
        None,
        &[],
        Actor::Human(human()),
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}\n\
             ## contents\n\
             - q1 \"Which key accepts a suggestion?\" \u{b7} uncited\n\
             \n\
             ## q1 \"Which key accepts a suggestion?\"\n\
             \n\
             - open\n",
            head()
        )
    );
}

#[test]
fn a_reopening_question_shows_under_the_decision_it_doubts() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(&mut map, "question", "Which model is default?", None, &[source], Actor::Human(human()));
    add(&mut map, "decision", "gpt3 by default", None, &[source], Actor::Human(human()));
    link(
        &mut map,
        "resolves",
        ("decision", "gpt3 by default"),
        ("question", "Which model is default?"),
    );
    add(&mut map, "question", "Does gpt3 still fit?", None, &[source], Actor::Human(human()));
    link(
        &mut map,
        REOPENS,
        ("question", "Does gpt3 still fit?"),
        ("decision", "gpt3 by default"),
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}{}\n\
             ## q1 \"Which model is default?\"\n\
             \n\
             - decision d1 \"gpt3 by default\"\n\
             \x20 reopened by q2 \"Does gpt3 still fit?\"\n\
             \n\
             ## q2 \"Does gpt3 still fit?\"\n\
             \n\
             - open\n",
            head(),
            contents(&[
                ("q1", "Which model is default?", source),
                ("q2", "Does gpt3 still fit?", source),
            ]),
        )
    );
}

#[test]
fn a_superseding_decision_shows_its_predecessor_as_was() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(
        &mut map,
        "question",
        "Which model is default?",
        None,
        &[source],
        Actor::Human(human()),
    );
    add(
        &mut map,
        "decision",
        "gpt3 by default",
        None,
        &[source],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "resolves",
        ("decision", "gpt3 by default"),
        ("question", "Which model is default?"),
    );
    add(
        &mut map,
        "decision",
        "gemma4 by default",
        None,
        &[source],
        Actor::Human(human()),
    );
    link(
        &mut map,
        SUPERSEDES,
        ("decision", "gemma4 by default"),
        ("decision", "gpt3 by default"),
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}{}\n\
             ## q1 \"Which model is default?\"\n\
             \n\
             - decision d2 \"gemma4 by default\"\n\
             \x20 was d1 \"gpt3 by default\"\n",
            head(),
            contents(&[("q1", "Which model is default?", source)]),
        )
    );
}

#[test]
fn a_supersession_chain_lists_every_predecessor_nearest_first() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(
        &mut map,
        "question",
        "Which model?",
        None,
        &[source],
        Actor::Human(human()),
    );
    for name in ["A", "B", "C"] {
        add(&mut map, "decision", name, None, &[source], Actor::Human(human()));
    }
    link(
        &mut map,
        "resolves",
        ("decision", "A"),
        ("question", "Which model?"),
    );
    link(&mut map, SUPERSEDES, ("decision", "B"), ("decision", "A"));
    link(&mut map, SUPERSEDES, ("decision", "C"), ("decision", "B"));

    assert_eq!(
        markdown(&map),
        format!(
            "{}{}\n\
             ## q1 \"Which model?\"\n\
             \n\
             - decision d3 \"C\"\n\
             \x20 was d2 \"B\"\n\
             \x20 was d1 \"A\"\n",
            head(),
            contents(&[("q1", "Which model?", source)]),
        )
    );
}

#[test]
fn a_decision_citing_a_different_prompt_than_its_question_names_the_source() {
    let mut map = Map::empty(decisions());
    let (raised, settled) = (EventId::new(), EventId::new());
    add(
        &mut map,
        "question",
        "Which model?",
        None,
        &[raised],
        Actor::Human(human()),
    );
    add(
        &mut map,
        "decision",
        "gemma4",
        None,
        &[settled],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "resolves",
        ("decision", "gemma4"),
        ("question", "Which model?"),
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}{}\n\
             ## q1 \"Which model?\"\n\
             \n\
             - decision d1 \"gemma4\"\n\
             \x20 source {}\n",
            head(),
            contents(&[("q1", "Which model?", raised)]),
            settled.as_uuid()
        )
    );
}

#[test]
fn a_model_written_question_is_marked() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(
        &mut map,
        "question",
        "Which key accepts a suggestion?",
        None,
        &[source],
        Actor::Agent,
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}{}\n\
             ## q1 \"Which key accepts a suggestion?\" (agent)\n\
             \n\
             - open\n",
            head(),
            contents(&[("q1", "Which key accepts a suggestion?", source)])
                .replace(" \u{b7} ", " (agent) \u{b7} "),
        )
    );
}

#[test]
fn a_decision_resolving_no_question_gets_its_own_h2() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(
        &mut map,
        "decision",
        "gemma4 by default",
        Some("the local model"),
        &[source],
        Actor::Agent,
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}{}\n\
             ## d1 \"gemma4 by default\" (agent)\n\
             \n\
             - decision d1 \"gemma4 by default\" (agent)\n\
             \x20 why: \"the local model\"\n",
            head(),
            contents(&[("d1", "gemma4 by default", source)])
                .replace(" \u{b7} ", " (agent) \u{b7} "),
        )
    );
}

/// Folds `events` into the decisions map - what a `changed by` test
/// needs, since `node.changed` carrying only `why` is not a
/// `Mutation` `add`/`link` builds through `Map::apply`.
fn folded(events: &[crate::core::Event]) -> Map {
    Map::fold(decisions(), &crate::core::testing::scope(), events).unwrap()
}

fn node_added(node: crate::core::NodeId, kind: &str, name: &str, why: Option<&str>) -> crate::core::Event {
    let properties = why
        .map(|why| BTreeMap::from([("why".to_string(), why.to_string())]))
        .unwrap_or_default();
    crate::core::Event::new(
        Actor::Agent,
        crate::core::testing::source("test"),
        None,
        crate::core::Payload::NodeAdded {
            map: "decisions".to_string(),
            node,
            kind: kind.to_string(),
            name: name.to_string(),
            properties,
            sources: Vec::new(),
            seq: 0,
        },
    )
}

fn node_changed(node: crate::core::NodeId, why: Option<&str>) -> crate::core::Event {
    crate::core::Event::new(
        Actor::Human(human()),
        crate::core::testing::source("test"),
        None,
        crate::core::Payload::NodeChanged {
            map: "decisions".to_string(),
            node,
            name: None,
            properties: BTreeMap::new(),
            sources: Vec::new(),
            why: why.map(str::to_string),
        },
    )
}

#[test]
fn a_changed_node_renders_its_changed_by_line_with_its_why() {
    let node = crate::core::NodeId::new();
    let map = folded(&[
        node_added(node, "decision", "gemma4 by default", Some("the local model")),
        node_changed(node, Some("never proposed")),
    ]);

    let text = markdown(&map);

    assert!(text.contains("changed by human: \"never proposed\"\n"), "{text}");
}

#[test]
fn a_changed_node_renders_its_changed_by_line_without_a_why() {
    let node = crate::core::NodeId::new();
    let map = folded(&[
        node_added(node, "decision", "gemma4 by default", Some("the local model")),
        node_changed(node, None),
    ]);

    let text = markdown(&map);

    assert!(text.contains("changed by human\n"), "{text}");
}

#[test]
fn an_unchanged_node_carries_no_changed_by_line() {
    let node = crate::core::NodeId::new();
    let map = folded(&[node_added(node, "decision", "gemma4 by default", None)]);

    let text = markdown(&map);

    assert!(!text.contains("changed by"), "{text}");
}

#[test]
fn a_question_lists_the_options_weighed_against_its_decision() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(
        &mut map,
        "question",
        "How is the provider built?",
        None,
        &[source],
        Actor::Human(human()),
    );
    add(
        &mut map,
        "decision",
        "its own wire parser",
        Some("a different SSE shape"),
        &[source],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "resolves",
        ("decision", "its own wire parser"),
        ("question", "How is the provider built?"),
    );
    add(
        &mut map,
        "option",
        "reuse the OpenAi struct",
        Some("the wire shapes differ"),
        &[source],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "answers",
        ("option", "reuse the OpenAi struct"),
        ("question", "How is the provider built?"),
    );

    assert!(markdown(&map).contains(
        "- decision d1 \"its own wire parser\"\n\
         \x20 why: \"a different SSE shape\"\n\
         - weighed o1 \"reuse the OpenAi struct\"\n\
         \x20 why: \"the wire shapes differ\"\n"
    ));
}

#[test]
fn an_option_that_repeats_the_winning_decision_is_not_listed_as_weighed() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(
        &mut map,
        "question",
        "Which parser?",
        None,
        &[source],
        Actor::Human(human()),
    );
    add(
        &mut map,
        "decision",
        "its own parser",
        None,
        &[source],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "resolves",
        ("decision", "its own parser"),
        ("question", "Which parser?"),
    );
    add(
        &mut map,
        "option",
        "its own parser",
        Some("older data"),
        &[source],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "answers",
        ("option", "its own parser"),
        ("question", "Which parser?"),
    );

    assert!(!markdown(&map).contains("- weighed "));
}

#[test]
fn an_option_with_no_answers_edge_stays_out_of_the_render() {
    let mut map = Map::empty(decisions());
    let source = EventId::new();
    add(
        &mut map,
        "question",
        "Which model?",
        None,
        &[source],
        Actor::Human(human()),
    );
    add(
        &mut map,
        "option",
        "a loose option",
        Some("no edge"),
        &[source],
        Actor::Human(human()),
    );

    let text = markdown(&map);

    assert!(!text.contains("- weighed "));
    assert!(!text.contains("a loose option"));
}

#[test]
fn a_resolves_edge_between_the_wrong_kinds_settles_nothing() {
    // Built from raw events, not `link` (`Map::apply`): a `resolves`
    // edge to an `option` end would now be refused on write. `replay`
    // never checks ends, so this shape still folds from history
    // written before the rule, and the render must still cope with it.
    let option = node_added_citing(Actor::Human(human()), "option", "gemma4", vec![]);
    let decision = node_added_citing(Actor::Human(human()), "decision", "gemma4 by default", vec![]);
    let edge = edge_added("resolves", &decision, &option);
    let map = folded(&[option, decision, edge]);

    assert!(markdown(&map).contains("## d1 \"gemma4 by default\"\n\n- decision d1 \"gemma4 by default\"\n"));
}

#[test]
fn a_map_with_no_question_or_decision_says_so() {
    let mut map = Map::empty(decisions());
    add(
        &mut map,
        "option",
        "Rust",
        Some("only alternative weighed"),
        &[EventId::new()],
        Actor::Human(human()),
    );

    assert_eq!(
        markdown(&map),
        format!(
            "{}\n(no question or decision yet; 1 nodes of other kinds.)\n",
            head()
        )
    );
}

#[test]
fn open_tasks_render_flat_with_why_and_blockers_then_by_state() {
    let mut map = Map::empty(tasks());
    let (first, second) = (EventId::new(), EventId::new());
    add(
        &mut map,
        "task",
        "send AGENTS.md to a coding turn",
        Some("the model never saw the rules"),
        &[first],
        Actor::Agent,
    );
    add(
        &mut map,
        "task",
        "cancel a turn without quitting",
        Some("Esc drops the session"),
        &[first],
        Actor::Human(human()),
    );
    add(
        &mut map,
        "task",
        "cancellable reply streams",
        Some("nothing can stop a stream today"),
        &[second],
        Actor::Human(human()),
    );
    link(
        &mut map,
        "blocks",
        ("task", "cancellable reply streams"),
        ("task", "cancel a turn without quitting"),
    );
    change(
        &mut map,
        "task",
        "send AGENTS.md to a coding turn",
        BTreeMap::from([
            ("state".to_string(), "done".to_string()),
            ("outcome".to_string(), "done in 1f1a9a9".to_string()),
        ]),
        Actor::Agent,
    );

    let expected = format!(
        "{}{}\n\
         ## t2 \"cancel a turn without quitting\"\n\
         \n\
         state: \"open\"\n\
         why: \"Esc drops the session\"\n\
         waits on t3 \"cancellable reply streams\"\n\
         \n\
         ## t3 \"cancellable reply streams\"\n\
         \n\
         state: \"open\"\n\
         why: \"nothing can stop a stream today\"\n\
         \n\
         ## done\n\
         - t1 \"send AGENTS.md to a coding turn\" (agent)\n\
         \x20 outcome: \"done in 1f1a9a9\"\n\
         \x20 state: \"done\"\n\
         \x20 why: \"the model never saw the rules\"\n\
         \x20 changed by agent\n",
        tasks_head(),
        contents(&[
            ("t2", "cancel a turn without quitting", first),
            ("t3", "cancellable reply streams", second),
        ]),
    );

    assert_eq!(markdown(&map), expected);
}

#[test]
fn a_dropped_task_renders_under_dropped_with_its_outcome() {
    let mut map = Map::empty(tasks());
    let first = EventId::new();
    add(
        &mut map,
        "task",
        "rewrite the render in wasm",
        Some("faster paint"),
        &[first],
        Actor::Human(human()),
    );
    change(
        &mut map,
        "task",
        "rewrite the render in wasm",
        BTreeMap::from([
            ("state".to_string(), "dropped".to_string()),
            ("outcome".to_string(), "not worth the build complexity".to_string()),
        ]),
        Actor::Human(human()),
    );

    let expected = format!(
        "{}\n\
         ## dropped\n\
         - t1 \"rewrite the render in wasm\"\n\
         \x20 outcome: \"not worth the build complexity\"\n\
         \x20 state: \"dropped\"\n\
         \x20 why: \"faster paint\"\n\
         \x20 changed by human\n",
        tasks_head(),
    );

    assert_eq!(markdown(&map), expected);
}

#[test]
fn an_empty_tasks_map_renders_its_guide_and_the_empty_notice() {
    assert_eq!(
        markdown(&Map::empty(tasks())),
        format!(
            "{}\n(empty: nothing has been recorded here yet.)\n",
            tasks_head()
        )
    );
}

#[test]
fn a_map_of_another_schema_renders_per_kind() {
    let mut map = Map::empty(files());
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
    link(
        &mut map,
        "contains",
        ("file", "src/main.rs"),
        ("function", "main"),
    );

    let expected = format!(
        "# files\n\
         \n\
         Folded from the percept log for this project and rerendered on every write. \
         Change it with `percept maps`, not by hand.\n\
         \n\
         ## file\n\
         - f1 \"src/main.rs\"\n\
         \n\
         ## function\n\
         - fn1 \"main\": returns: \"()\"\n\
         \x20 sources: {}\n\
         \n\
         ## edges\n\
         - file \"src/main.rs\" contains function \"main\"\n",
        cited.as_uuid()
    );

    assert_eq!(markdown(&map), expected);
}

#[test]
fn the_catalogue_gives_each_map_a_section_with_its_kinds_glossed() {
    let mut map = Map::empty(decisions());
    add(
        &mut map,
        "question",
        "Where does the log live?",
        None,
        &[],
        Actor::Human(human()),
    );

    let text = catalogue(std::slice::from_ref(&map));

    assert!(text.starts_with("# maps\n"));
    assert!(text.contains("## decisions\n"));
    assert!(text.contains(&decisions().purpose));
    assert!(text.contains("1 nodes, 0 edges.\n"));
    assert!(text.contains("\nNode kinds:\n- `question` - a matter the project had to settle\n"));
    assert!(text.contains(
        "\nEdge kinds:\n- `answers` (option -> question) - from an option to the question"
    ));
    assert!(text.contains("\nExample node and edge:\n\n    {\"node\":"));
    assert!(text.contains("\"name\":\"Where does the log live?\""));
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
