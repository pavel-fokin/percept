//! A map's Markdown, and where it lands on disk. `markdown` is the
//! pure text, kept separate from `MarkdownFiles` so it is testable
//! without touching a filesystem. `MarkdownFiles` implements
//! `percept::MapRenderer`.

use std::fs;
use std::path::PathBuf;

use crate::percept::{Map, MapRenderer, Node, Schema};
use crate::store::event::ids;

/// What every rendered map opens with, so a reader who lands on the
/// file the way they'd land on a README knows not to hand-edit it.
const PREAMBLE: &str = "Folded from the percept log for this project and rerendered on every \
    write. Change it with `percept maps`, not by hand.";

/// `map` as Markdown: a heading and the preamble, then one `## <kind>`
/// section per node kind that holds a node - headline kinds first,
/// then the schema's remaining kinds - and a `## edges` section when
/// the map has any. Empty for a map with no nodes, past the preamble.
pub fn markdown(map: &Map) -> String {
    let schema = map.schema();
    let mut out = format!("# {}\n\n{PREAMBLE}\n", schema.name);

    if map.nodes().is_empty() {
        out.push_str("\n(empty: nothing has been recorded here yet.)\n");
        return out;
    }

    for kind in ordered_kinds(schema) {
        let nodes: Vec<&Node> = map
            .nodes()
            .iter()
            .filter(|node| node.kind == kind)
            .collect();
        if nodes.is_empty() {
            continue;
        }
        out.push_str("\n## ");
        out.push_str(kind);
        out.push('\n');
        for node in nodes {
            push_node(&mut out, node);
        }
    }

    if !map.edges().is_empty() {
        out.push_str("\n## edges\n");
        for edge in map.edges() {
            out.push_str(&format!(
                "- {} {} {}\n",
                map.label(edge.from),
                edge.kind,
                map.label(edge.to)
            ));
        }
    }

    out
}

/// Headline kinds first, in `headline_kinds` order, then the rest of
/// `node_kinds` in schema order - the order a reader wants a map's
/// sections in.
fn ordered_kinds(schema: &'static Schema) -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = schema.headline_kinds.to_vec();
    for kind in schema.node_kinds {
        if !kinds.contains(kind) {
            kinds.push(kind);
        }
    }
    kinds
}

/// One node's bullet - its name and properties the way `Map`'s
/// `Display` writes a node line, minus the leading kind - then, on its
/// own indented line, the sources it cites, when it cites any.
fn push_node(out: &mut String, node: &Node) {
    out.push_str(&format!("- {:?}", node.name));
    let mut sep = ": ";
    for (key, value) in &node.properties {
        out.push_str(&format!("{sep}{key}: {value:?}"));
        sep = "; ";
    }
    out.push('\n');
    if !node.sources.is_empty() {
        out.push_str("  sources: ");
        out.push_str(&ids(&node.sources).join(", "));
        out.push('\n');
    }
}

/// Renders a map to `<dir>/<schema name>.md`, replacing whatever was
/// there. Implements `percept::MapRenderer`.
pub struct MarkdownFiles {
    dir: PathBuf,
}

impl MarkdownFiles {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl MapRenderer for MarkdownFiles {
    /// Creates `dir` if it is missing, so the first write to a fresh
    /// project doesn't need `mkdir -p .percept` first.
    fn render(&self, map: &Map) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&self.dir)?;
        fs::write(
            self.dir.join(format!("{}.md", map.schema().name)),
            markdown(map),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
