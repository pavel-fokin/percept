//! A map's Markdown, and where it lands on disk. `markdown` is the
//! pure text, kept separate from `MarkdownFiles` so it is testable
//! without touching a filesystem. `MarkdownFiles` implements
//! `percept::MapRenderer`.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use crate::percept::{Edge, Map, MapRenderer, Node, NodeId, Schema, DECISIONS, DERIVED};
use crate::store::event::ids;

/// What every rendered map opens with, so a reader who lands on the
/// file the way they'd land on a README knows not to hand-edit it.
const PREAMBLE: &str = "Folded from the percept log for this project and rerendered on every \
    write. Change it with `percept maps`, not by hand.";

/// Decisions use explicit commitment groups. Other schemas retain
/// sections by kind; neither view infers a relationship.
pub fn markdown(map: &Map) -> String {
    let schema = map.schema();
    let derived = DERIVED.contains(&schema);
    let preamble = if derived {
        "Built from the current working tree. Change the code, then read this map again."
    } else {
        PREAMBLE
    };
    let mut out = format!("# {}\n\n{preamble}\n", schema.name);

    if map.nodes().is_empty() {
        out.push_str(if derived {
            "\n(empty: no code nodes were found.)\n"
        } else {
            "\n(empty: nothing has been recorded here yet.)\n"
        });
        return out;
    }

    if schema == &DECISIONS {
        push_decisions(&mut out, map);
        out.truncate(out.trim_end().len());
        out.push('\n');
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
            let _ = writeln!(out, "- {}", map.edge_line(edge));
            if !edge.sources.is_empty() {
                let _ = writeln!(out, "  sources: {}", ids(&edge.sources).join(", "));
            }
        }
    }

    out
}

fn push_decisions(out: &mut String, map: &Map) {
    let headlines: Vec<&Node> = map.headlines().collect();
    out.push_str(
        "\n## Overview\n\nShared interpretations; agreement is unknown unless cited explicitly. \
        Open an entry for its rationale, supporting choices, and relationships.\n\n<ul>\n",
    );
    for node in &headlines {
        let _ = writeln!(out, "<li>{}</li>", node_link(node));
    }
    out.push_str("</ul>\n\n");

    let mut anchored: HashSet<NodeId> = headlines.iter().map(|node| node.id).collect();
    let mut shown_nodes = HashSet::new();
    let mut shown_edges = HashSet::new();
    for headline in headlines {
        let mut members = HashSet::from([headline.id]);
        if headline.kind == "commitment" {
            members.extend(
                map.edges()
                    .iter()
                    .filter(|edge| edge.kind == "details" && edge.from == headline.id)
                    .map(|edge| edge.to),
            );
            let questions: Vec<NodeId> = map
                .edges()
                .iter()
                .filter(|edge| {
                    edge.kind == "resolves"
                        && members.contains(&edge.from)
                        && map
                            .node(edge.from)
                            .is_some_and(|node| node.kind == "decision")
                        && map
                            .node(edge.to)
                            .is_some_and(|node| node.kind == "question")
                })
                .map(|edge| edge.to)
                .collect();
            members.extend(questions);
        }
        let _ = writeln!(
            out,
            "<details id=\"node-{}\">\n<summary>{}: {}</summary>\n",
            headline.id.as_uuid(),
            escape(&headline.kind),
            escape(&headline.name)
        );
        push_html_node(out, headline, false);
        for node in map
            .nodes()
            .iter()
            .filter(|node| members.contains(&node.id) && node.id != headline.id)
        {
            push_html_node(out, node, anchored.insert(node.id));
        }
        for (index, edge) in map
            .edges()
            .iter()
            .enumerate()
            .filter(|(_, edge)| members.contains(&edge.from) || members.contains(&edge.to))
        {
            push_html_edge(out, map, edge);
            shown_edges.insert(index);
        }
        shown_nodes.extend(members);
        out.push_str("\n</details>\n\n");
    }

    let other_nodes: Vec<&Node> = map
        .nodes()
        .iter()
        .filter(|node| !shown_nodes.contains(&node.id))
        .collect();
    let other_edges: Vec<&Edge> = map
        .edges()
        .iter()
        .enumerate()
        .filter(|(index, _)| !shown_edges.contains(index))
        .map(|(_, edge)| edge)
        .collect();
    if !other_nodes.is_empty() || !other_edges.is_empty() {
        let _ = writeln!(
            out,
            "<details>\n<summary>Other recorded material ({} nodes, {} relationships)</summary>\n",
            other_nodes.len(),
            other_edges.len()
        );
        for node in other_nodes {
            push_html_node(out, node, anchored.insert(node.id));
        }
        for edge in other_edges {
            push_html_edge(out, map, edge);
        }
        out.push_str("\n</details>\n");
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn node_link(node: &Node) -> String {
    format!(
        "<a href=\"#node-{}\">{}: {}</a>",
        node.id.as_uuid(),
        escape(&node.kind),
        escape(&node.name)
    )
}

fn push_html_node(out: &mut String, node: &Node, anchor: bool) {
    let id = if anchor {
        format!(" id=\"node-{}\"", node.id.as_uuid())
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        "<div{id}>\n<p><strong>{}: {}</strong></p>",
        escape(&node.kind),
        escape(&node.name)
    );
    if !node.properties.is_empty() {
        out.push_str("<dl>\n");
        for (key, value) in &node.properties {
            let _ = writeln!(out, "<dt>{}</dt><dd>{}</dd>", escape(key), escape(value));
        }
        out.push_str("</dl>\n");
    }
    if !node.sources.is_empty() {
        let _ = writeln!(
            out,
            "<p>Node sources: <code>{}</code></p>",
            ids(&node.sources).join(", ")
        );
    }
    out.push_str("</div>\n");
}

fn push_html_edge(out: &mut String, map: &Map, edge: &Edge) {
    let from = map
        .node(edge.from)
        .expect("an edge's ends are nodes of its map");
    let to = map
        .node(edge.to)
        .expect("an edge's ends are nodes of its map");
    let _ = writeln!(
        out,
        "<p>{} <strong>{}</strong> {}</p>",
        node_link(from),
        escape(&edge.kind),
        node_link(to)
    );
    if !edge.sources.is_empty() {
        let _ = writeln!(
            out,
            "<p>Relationship sources: <code>{}</code></p>",
            ids(&edge.sources).join(", ")
        );
    }
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

/// One node's bullet - its name and properties, the kind being the
/// section's - then, on its own indented line, the sources it cites,
/// when it cites any.
fn push_node(out: &mut String, node: &Node) {
    let _ = writeln!(out, "- {:?}{}", node.name, node.properties_line());
    if !node.sources.is_empty() {
        let _ = writeln!(out, "  sources: {}", ids(&node.sources).join(", "));
    }
}

/// Renders a map to `<dir>/<schema name>.md`, replacing whatever was
/// there.
pub struct MarkdownFiles {
    dir: PathBuf,
}

impl MarkdownFiles {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl MapRenderer for MarkdownFiles {
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
