use std::collections::BTreeMap;

use uuid::Uuid;

use super::*;
use crate::percept::{EventId, Mutation, NodeRef, Schema};

fn decisions() -> &'static Schema {
    Schema::find("decisions").unwrap()
}

fn node_ref(kind: &str, name: &str) -> NodeRef {
    NodeRef {
        kind: kind.to_string(),
        name: name.to_string(),
    }
}

#[test]
fn an_empty_map_renders_the_heading_the_preamble_and_the_empty_notice() {
    let text = markdown(&Map::empty(decisions()));

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
fn a_map_with_headlines_a_property_a_source_and_an_edge_renders_in_sections() {
    let mut map = Map::empty(decisions());
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

    let expected = format!(
        "# decisions\n\
         \n\
         Folded from the percept log for this project and rerendered on every write. \
         Change it with `percept maps`, not by hand.\n\
         \n\
         ## question\n\
         - \"Which language?\"\n\
         \n\
         ## decision\n\
         - \"Rust over Go\": rationale: \"faster\"\n\
         \x20 sources: {}\n\
         \n\
         ## option\n\
         - \"Rust\"\n\
         \n\
         ## edges\n\
         - decision \"Rust over Go\" resolves question \"Which language?\"\n",
        cited.as_uuid()
    );

    assert_eq!(markdown(&map), expected);
}

/// The seed committed at `.percept/decisions.md` and what a fresh
/// render produces must never drift.
#[test]
fn the_empty_map_matches_the_committed_seed_file() {
    assert_eq!(
        markdown(&Map::empty(decisions())),
        include_str!("../../../.percept/decisions.md")
    );
}

/// A directory under the system temp dir, removed when the test ends -
/// a trailing `remove_dir_all` never runs on a failing test, which is
/// the one whose files you'd want left in place.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("percept-render-{}", Uuid::now_v7())),
        }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn markdown_files_writes_the_map_named_file_in_its_directory_creating_it() {
    let dir = TempDir::new();
    assert!(!dir.path.exists());
    let renderer = MarkdownFiles::new(&dir.path);
    let map = Map::empty(decisions());

    renderer.render(&map).unwrap();

    let written = fs::read_to_string(dir.path.join("decisions.md")).unwrap();
    assert_eq!(written, markdown(&map));
}
