use std::collections::BTreeMap;

use super::*;
use crate::percept::{EventId, Mutation, DECISIONS};
use crate::testing::node_ref;

#[test]
fn an_empty_map_renders_the_heading_the_preamble_and_the_empty_notice() {
    let text = markdown(&Map::empty(&DECISIONS));

    assert_eq!(
        text,
        "# decisions\n\
         \n\
         Folded from the percept log for this project and rerendered on every write. \
         Change it with `percept maps`, not by hand.\n\
         \n\
         (empty: nothing has been recorded here yet.)\n"
    );
}

#[test]
fn a_legacy_decision_keeps_its_rationale_sources_and_ungrouped_material() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(Mutation::AddNode {
        kind: "question".to_string(),
        name: "Which language?".to_string(),
        properties: BTreeMap::new(),
        sources: Vec::new(),
    })
    .unwrap();
    let cited = EventId::new();
    map.apply(Mutation::AddNode {
        kind: "decision".to_string(),
        name: "Rust over Go".to_string(),
        properties: BTreeMap::from([("rationale".to_string(), "faster".to_string())]),
        sources: vec![cited],
    })
    .unwrap();
    map.apply(Mutation::AddNode {
        kind: "option".to_string(),
        name: "Rust".to_string(),
        properties: BTreeMap::new(),
        sources: Vec::new(),
    })
    .unwrap();
    map.apply(Mutation::AddEdge {
        kind: "resolves".to_string(),
        from: node_ref("decision", "Rust over Go"),
        to: node_ref("question", "Which language?"),
        sources: Vec::new(),
    })
    .unwrap();

    let rendered = markdown(&map);
    assert!(rendered.contains("## Overview"));
    assert!(rendered.contains("<summary>decision: Rust over Go</summary>"));
    assert!(rendered.contains("<dt>rationale</dt><dd>faster</dd>"));
    assert!(rendered.contains(&cited.as_uuid().to_string()));
    assert!(rendered.contains("<strong>resolves</strong>"));
    assert!(rendered.contains("Other recorded material (1 nodes, 0 relationships)"));
    assert!(rendered.contains("option: Rust"));
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

fn grouped_map() -> (Map, EventId) {
    let mut map = Map::empty(&DECISIONS);
    let cited = EventId::new();
    for (kind, name) in [
        ("commitment", "Storage"),
        ("decision", "JSONL"),
        ("question", "Where?"),
        ("evidence", "An exception"),
        ("option", "Unrelated"),
    ] {
        map.apply(Mutation::AddNode {
            kind: kind.into(),
            name: name.into(),
            properties: BTreeMap::from([("why".into(), "Keep evidence".into())]),
            sources: vec![cited],
        })
        .unwrap();
    }
    for (kind, from, to) in [
        (
            "details",
            node_ref("commitment", "Storage"),
            node_ref("decision", "JSONL"),
        ),
        (
            "resolves",
            node_ref("decision", "JSONL"),
            node_ref("question", "Where?"),
        ),
        (
            "contradicts",
            node_ref("evidence", "An exception"),
            node_ref("decision", "JSONL"),
        ),
    ] {
        map.apply(Mutation::AddEdge {
            kind: kind.into(),
            from,
            to,
            sources: vec![cited],
        })
        .unwrap();
    }
    (map, cited)
}

#[test]
fn grouping_preserves_every_node_anchor_and_relationship_source() {
    let (map, cited) = grouped_map();
    let text = markdown(&map);
    for node in map.nodes() {
        assert_eq!(
            text.matches(&format!("id=\"node-{}\"", node.id.as_uuid()))
                .count(),
            1
        );
        assert!(text.contains(&format!("<strong>{}: {}</strong>", node.kind, node.name)));
    }
    assert!(text.contains(&format!(
        "Relationship sources: <code>{}</code>",
        cited.as_uuid()
    )));
    assert!(text.contains("<strong>contradicts</strong>"));
    assert!(text.contains("Other recorded material (2 nodes, 0 relationships)"));
}

#[test]
fn grouping_places_a_decision_and_its_question_under_one_commitment() {
    let (map, _) = grouped_map();
    let text = markdown(&map);
    assert_eq!(text.matches("<li>").count(), 1);
    let details = text
        .split("<summary>commitment: Storage</summary>")
        .nth(1)
        .unwrap()
        .split("</details>")
        .next()
        .unwrap();
    assert!(details.contains("decision: JSONL"));
    assert!(details.contains("question: Where?"));
    assert!(details.contains("evidence: An exception"));
}

#[test]
fn recorded_text_cannot_inject_html_into_the_decision_view() {
    let mut map = Map::empty(&DECISIONS);
    map.apply(Mutation::AddNode {
        kind: "commitment".into(),
        name: "</summary><script>run()</script>".into(),
        properties: BTreeMap::from([("<img>".into(), "<script>&\"".into())]),
        sources: vec![],
    })
    .unwrap();
    let text = markdown(&map);
    assert!(!text.contains("<script>"));
    assert!(!text.contains("<img>"));
    assert!(text.contains("&lt;script&gt;&amp;&quot;"));
}

#[test]
fn unrelated_additions_do_not_change_existing_node_anchors() {
    let (mut map, _) = grouped_map();
    let before: Vec<_> = map
        .nodes()
        .iter()
        .map(|n| format!("id=\"node-{}\"", n.id.as_uuid()))
        .collect();
    map.apply(Mutation::AddNode {
        kind: "commitment".into(),
        name: "Another commitment".into(),
        properties: BTreeMap::new(),
        sources: vec![],
    })
    .unwrap();
    let text = markdown(&map);
    for anchor in before {
        assert_eq!(text.matches(&anchor).count(), 1);
    }
}

#[test]
fn a_code_export_identifies_its_working_tree_origin() {
    let text = markdown(&Map::empty(&crate::percept::CODE));
    assert!(text.contains("Built from the current working tree"));
    assert!(!text.contains("Folded from the percept log"));
}
