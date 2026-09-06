//! A map's Markdown, and where it lands on disk. `markdown` is the
//! pure text, kept separate from `MarkdownFiles` so it is testable
//! without touching a filesystem. `MarkdownFiles` implements
//! `percept::MapRenderer`.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use crate::percept::{Actor, EventId, Map, MapRenderer, Node, NodeId, Schema, DECISIONS};
use crate::store::event::ids;

/// What every rendered map opens with, so a reader who lands on the
/// file the way they'd land on a README knows not to hand-edit it.
const PREAMBLE: &str = "Folded from the percept log for this project and rerendered on every \
    write. Change it with `percept maps`, not by hand.";

/// What the decisions map opens with: the same notice, plus how the
/// list below reads and where the detail it leaves out still lives.
const DECISIONS_PREAMBLE: &str = "Folded from the percept log for this project and rerendered \
    on every write. Change it with `percept maps`, not by hand. Questions in the order they \
    were raised, grouped under the prompt that settled them, each with its decision. Options \
    and evidence: `percept maps show decisions --around 'question:<name>'`. What changed \
    lately: `percept maps show decisions --since 1d`.";

/// `map` as Markdown: a heading and the preamble, then the map's body.
/// The decisions map renders as a question-keyed list grouped by the
/// prompt that settled each question - see `push_decisions`. Every
/// other schema keeps one `## <kind>` section per node kind that holds
/// a node and a `## edges` section - see `push_by_kind`. Empty for a
/// map with no nodes, past the preamble.
pub fn markdown(map: &Map) -> String {
    let schema = map.schema();
    let preamble = if schema.name == DECISIONS.name {
        DECISIONS_PREAMBLE
    } else {
        PREAMBLE
    };
    let mut out = format!("# {}\n\n{preamble}\n", schema.name);

    if map.nodes().is_empty() {
        out.push_str("\n(empty: nothing has been recorded here yet.)\n");
        return out;
    }

    if schema.name == DECISIONS.name {
        push_decisions(&mut out, map);
    } else {
        push_by_kind(&mut out, map);
    }

    out
}

/// One `## <kind>` section per node kind that holds a node - headline
/// kinds first, then the schema's remaining kinds - then a `## edges`
/// section when the map has any.
fn push_by_kind(out: &mut String, map: &Map) {
    for kind in ordered_kinds(map.schema()) {
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
            push_node(out, node);
        }
    }

    if !map.edges().is_empty() {
        out.push_str("\n## edges\n");
        for edge in map.edges() {
            let _ = writeln!(out, "- {}", map.edge_line(edge));
        }
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

/// The decisions map's body: every question, and every decision that
/// resolves none, grouped under the prompt that settled it and listed
/// in the order it was raised. Under a question: its decision, or
/// `open`; under a decision: what it superseded, as `was`.
fn push_decisions(out: &mut String, map: &Map) {
    let mut resolved_by: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut resolvers: HashSet<NodeId> = HashSet::new();
    for edge in map.edges().iter().filter(|edge| edge.kind == "resolves") {
        resolved_by.entry(edge.to).or_default().push(edge.from);
        resolvers.insert(edge.from);
    }

    let items = map.nodes().iter().filter(|node| {
        node.kind == "question"
            || node.kind == "decision"
                && !resolvers.contains(&node.id)
                && !map.is_superseded(node.id)
    });

    let mut group_order: Vec<Option<EventId>> = Vec::new();
    let mut groups: HashMap<Option<EventId>, Vec<&Node>> = HashMap::new();
    for item in items {
        let key = item.sources.first().copied();
        if !groups.contains_key(&key) {
            group_order.push(key);
        }
        groups.entry(key).or_default().push(item);
    }

    for key in group_order {
        out.push_str("\n## ");
        push_group_heading(out, key);
        out.push('\n');
        for node in &groups[&key] {
            if node.kind == "question" {
                push_question(out, map, node, &resolved_by);
            } else {
                push_decision_line(out, map, node, "- decision ");
            }
        }
    }
}

/// A group's heading: the settling prompt's date and id, the id alone
/// when it isn't a UUIDv7, or `uncited` when the question cites no
/// source at all.
fn push_group_heading(out: &mut String, key: Option<EventId>) {
    match key {
        None => out.push_str("uncited"),
        Some(id) => match id.minted_at() {
            Some(at) => {
                let _ = write!(out, "{} \u{b7} {}", at.date(), id.as_uuid());
            }
            None => {
                let _ = write!(out, "{}", id.as_uuid());
            }
        },
    }
}

/// One question's bullet, then its resolving decision - or `open` when
/// none resolves it, past what `is_superseded` has already ruled out.
fn push_question(
    out: &mut String,
    map: &Map,
    question: &Node,
    resolved_by: &HashMap<NodeId, Vec<NodeId>>,
) {
    let _ = writeln!(
        out,
        "- {}{}",
        marked_name(question),
        question.properties_line()
    );
    let decisions: Vec<&Node> = resolved_by
        .get(&question.id)
        .into_iter()
        .flatten()
        .filter(|id| !map.is_superseded(**id))
        .filter_map(|id| map.node(*id))
        .collect();
    if decisions.is_empty() {
        out.push_str("  open\n");
        return;
    }
    for decision in decisions {
        push_decision_line(out, map, decision, "  decision ");
    }
}

/// A decision's line under `prefix` - a question's indent, or its own
/// bullet when it resolves no question - then one `was` line per node
/// it supersedes.
fn push_decision_line(out: &mut String, map: &Map, decision: &Node, prefix: &str) {
    let _ = writeln!(
        out,
        "{prefix}{}{}",
        marked_name(decision),
        decision.properties_line()
    );
    for was in map.supersedes(decision.id) {
        let _ = writeln!(out, "  was {}", marked_name(was));
    }
}

/// A node's quoted name, marked `(model)` when the model wrote it -
/// `Actor::User` and `Actor::System` are unmarked.
fn marked_name(node: &Node) -> String {
    let mut label = format!("{:?}", node.name);
    if matches!(node.actor, Actor::Model) {
        label.push_str(" (model)");
    }
    label
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
