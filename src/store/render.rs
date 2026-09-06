//! A map's Markdown, and where it lands on disk. `markdown` is the
//! pure text, kept separate from `MarkdownFiles` so it is testable
//! without touching a filesystem. `MarkdownFiles` implements
//! `percept::MapRenderer`.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use crate::percept::{Actor, EventId, Map, MapRenderer, Node, NodeId, Schema, DECISIONS, RESOLVES};
use crate::store::event::ids;

/// What every rendered map opens with, so a reader who lands on the
/// file the way they'd land on a README knows not to hand-edit it.
const PREAMBLE: &str = "Folded from the percept log for this project and rerendered on every \
    write. Change it with `percept maps`, not by hand.";

/// What the decisions map adds to the preamble: how the list below
/// reads and where the detail it leaves out still lives.
const DECISIONS_GUIDE: &str = "Questions in the order they were raised, grouped under the \
    prompt that raised them, each with its decision. Options and evidence: `percept maps show \
    decisions --around 'question:<name>'`. What changed lately: `percept maps show decisions \
    --since 1d`.";

/// The decisions map's preamble: the shared notice, then its guide.
pub fn decisions_preamble() -> String {
    format!("{PREAMBLE} {DECISIONS_GUIDE}")
}

/// `map` as Markdown: a heading and the preamble, then the map's body.
/// The decisions map renders as a question-keyed list grouped by the
/// prompt that raised each question - see `push_decisions`. Every
/// other schema keeps one `## <kind>` section per node kind that holds
/// a node and a `## edges` section - see `push_by_kind`. Empty for a
/// map with no nodes, past the preamble.
pub fn markdown(map: &Map) -> String {
    let schema = map.schema();
    let decisions = schema.name == DECISIONS.name;
    let preamble = if decisions {
        decisions_preamble()
    } else {
        PREAMBLE.to_string()
    };
    let mut out = format!("# {}\n\n{preamble}\n", schema.name);

    if map.nodes().is_empty() {
        out.push_str("\n(empty: nothing has been recorded here yet.)\n");
        return out;
    }

    if decisions {
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
/// resolves none, grouped under the prompt that raised it - the first
/// source it cites, which never changes - and listed in the order it
/// was raised. Under a question: the decision that settles it now, or
/// `open`; under a decision: what it superseded, as `was`, and the
/// prompt it cites when that is not the group's.
fn push_decisions(out: &mut String, map: &Map) {
    // A question is settled by the current end of each resolver's
    // supersession chain, so a correction needs no new `resolves` edge.
    // A `resolves` edge between the wrong kinds settles nothing.
    let mut settled_by: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut settling: HashSet<NodeId> = HashSet::new();
    for edge in map.edges().iter().filter(|edge| edge.kind == RESOLVES) {
        let ends = map.node(edge.from).zip(map.node(edge.to));
        let Some((decision, question)) = ends.filter(|(d, q)| d.kind == DECISION && q.kind == QUESTION)
        else {
            continue;
        };
        let current = map.successor(decision.id);
        settling.extend([decision.id, current]);
        let entry = settled_by.entry(question.id).or_default();
        if !entry.contains(&current) {
            entry.push(current);
        }
    }

    let items: Vec<&Node> = map
        .nodes()
        .iter()
        .filter(|node| {
            node.kind == QUESTION
                || node.kind == DECISION
                    && !settling.contains(&node.id)
                    && !map.is_superseded(node.id)
        })
        .collect();
    if items.is_empty() {
        let _ = writeln!(
            out,
            "\n(no question or decision yet; {} nodes of other kinds.)",
            map.nodes().len()
        );
        return;
    }

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
            if node.kind == QUESTION {
                push_question(out, map, node, &settled_by, key);
            } else {
                push_decision_line(out, map, node, "- decision ", key);
            }
        }
    }
}

const QUESTION: &str = "question";
const DECISION: &str = "decision";

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

/// One question's bullet, then the decision that settles it now - or
/// `open` when none does.
fn push_question(
    out: &mut String,
    map: &Map,
    question: &Node,
    settled_by: &HashMap<NodeId, Vec<NodeId>>,
    group: Option<EventId>,
) {
    let _ = writeln!(
        out,
        "- {}{}",
        marked_name(question),
        question.properties_line()
    );
    let decisions: Vec<&Node> = settled_by
        .get(&question.id)
        .into_iter()
        .flatten()
        .filter_map(|id| map.node(*id))
        .collect();
    if decisions.is_empty() {
        out.push_str("  open\n");
        return;
    }
    for decision in decisions {
        push_decision_line(out, map, decision, "  decision ", group);
    }
}

/// A decision's line under `prefix` - a question's indent, or its own
/// bullet when it resolves no question - then the prompt it cites when
/// that is not the group's, then one `was` line per decision it
/// superseded, nearest first.
fn push_decision_line(
    out: &mut String,
    map: &Map,
    decision: &Node,
    prefix: &str,
    group: Option<EventId>,
) {
    let _ = writeln!(
        out,
        "{prefix}{}{}",
        marked_name(decision),
        decision.properties_line()
    );
    if let Some(source) = decision.sources.first().filter(|source| Some(**source) != group) {
        let _ = writeln!(out, "  source {}", source.as_uuid());
    }
    for was in map.predecessors(decision.id) {
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
