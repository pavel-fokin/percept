//! A map's Markdown, and where it lands on disk. `markdown` is the
//! pure text, kept separate from `MarkdownFiles` so it is testable
//! without touching a filesystem. `MarkdownFiles` implements
//! `percept::MapRenderer`.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use crate::percept::{Actor, EventId, Map, MapRenderer, Node, Schema, DECISIONS, TASKS};
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

/// What the tasks map adds to the preamble.
const TASKS_GUIDE: &str = "Open tasks in the order they were raised, grouped under the \
    prompt that raised them, each with what it waits on. Done and dropped tasks follow under \
    `done`, each with its outcome. A task's own history: `percept maps show tasks --around \
    'task:<name>'`. What changed lately: `percept maps show tasks --since 1d`.";

/// `map` as Markdown: a heading and the preamble, then the map's body.
/// The decisions map renders as a question-keyed list grouped by the
/// prompt that raised each question - see `push_decisions`; the tasks
/// map as open tasks grouped the same way, then the done ones - see
/// `push_tasks`. Every other schema keeps one `## <kind>` section per
/// node kind that holds a node and a `## edges` section - see
/// `push_by_kind`. Empty for a map with no nodes, past the preamble.
pub fn markdown(map: &Map) -> String {
    let schema = map.schema();
    let mut out = format!("# {}\n\n{PREAMBLE}", schema.name);
    let guide = match schema.name {
        name if name == DECISIONS.name => Some(DECISIONS_GUIDE),
        name if name == TASKS.name => Some(TASKS_GUIDE),
        _ => None,
    };
    if let Some(guide) = guide {
        out.push(' ');
        out.push_str(guide);
    }
    out.push('\n');

    if map.nodes().is_empty() {
        out.push_str("\n(empty: nothing has been recorded here yet.)\n");
        return out;
    }

    match schema.name {
        name if name == DECISIONS.name => push_decisions(&mut out, map),
        name if name == TASKS.name => push_tasks(&mut out, map),
        _ => push_by_kind(&mut out, map),
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
    let _ = writeln!(out, "- {}{}", marked_name(node), node.properties_line());
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
    // The headlines are the questions and the current decisions; a
    // decision that settles a question is shown under it, not on its own.
    let items = map
        .headlines()
        .filter(|node| node.kind == "question" || !map.settles(node.id));

    let groups = grouped_by_prompt(items);
    if groups.is_empty() {
        let _ = writeln!(
            out,
            "\n(no question or decision yet; {} nodes of other kinds.)",
            map.nodes().len()
        );
        return;
    }

    for (key, nodes) in groups {
        out.push_str("\n## ");
        push_group_heading(out, key);
        out.push('\n');
        for node in nodes {
            if node.kind == "question" {
                push_question(out, map, node, key);
            } else {
                push_decision_line(out, map, node, "- decision ", key);
            }
        }
    }
}

/// The tasks map's body: every open task - one no outcome resolves -
/// grouped under the prompt that raised it, in the order raised, each
/// with the tasks it waits on; then, under `done`, every settled task
/// with its outcome, in the order raised. A task never moves within
/// the open list; settling it is the one move, and the agreed one.
fn push_tasks(out: &mut String, map: &Map) {
    let (done, open): (Vec<&Node>, Vec<&Node>) = map
        .headlines()
        .partition(|task| !map.settled_by(task.id).is_empty());

    let groups = grouped_by_prompt(open.into_iter());
    if groups.is_empty() && done.is_empty() {
        let _ = writeln!(
            out,
            "\n(no task yet; {} nodes of other kinds.)",
            map.nodes().len()
        );
        return;
    }
    for (key, tasks) in groups {
        out.push_str("\n## ");
        push_group_heading(out, key);
        out.push('\n');
        for task in tasks {
            let _ = writeln!(out, "- {}{}", marked_name(task), task.properties_line());
            for blocker in map.blocked_by(task.id) {
                let _ = writeln!(out, "  waits on {}", marked_name(blocker));
            }
        }
    }
    if !done.is_empty() {
        out.push_str("\n## done\n");
        for task in done {
            let _ = writeln!(out, "- {}", marked_name(task));
            for outcome in map.settled_by(task.id) {
                let _ = writeln!(
                    out,
                    "  outcome {}{}",
                    marked_name(outcome),
                    outcome.properties_line()
                );
            }
        }
    }
}

/// `items` bucketed by the prompt each cites first - the one that
/// raised it, which never changes - in first-seen order of both the
/// prompts and the items under them.
fn grouped_by_prompt<'a>(
    items: impl Iterator<Item = &'a Node>,
) -> Vec<(Option<EventId>, Vec<&'a Node>)> {
    let mut groups: Vec<(Option<EventId>, Vec<&Node>)> = Vec::new();
    for item in items {
        let key = item.sources.first().copied();
        match groups.iter_mut().find(|(group, _)| *group == key) {
            Some((_, nodes)) => nodes.push(item),
            None => groups.push((key, vec![item])),
        }
    }
    groups
}

/// A group's heading: the raising prompt's date and id, the id alone
/// when it isn't a UUIDv7, or `uncited` when the question cites no
/// source at all.
fn push_group_heading(out: &mut String, key: Option<EventId>) {
    let Some(id) = key else {
        out.push_str("uncited");
        return;
    };
    if let Some(at) = id.minted_at() {
        let _ = write!(out, "{} \u{b7} ", at.date());
    }
    let _ = write!(out, "{}", id.as_uuid());
}

/// One question's bullet, then the decision that settles it now - or
/// `open` when none does.
fn push_question(out: &mut String, map: &Map, question: &Node, group: Option<EventId>) {
    let _ = writeln!(
        out,
        "- {}{}",
        marked_name(question),
        question.properties_line()
    );
    let decisions = map.settled_by(question.id);
    if decisions.is_empty() {
        out.push_str("  open\n");
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
    if let Some(source) = decision
        .sources
        .first()
        .filter(|source| Some(**source) != group)
    {
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
