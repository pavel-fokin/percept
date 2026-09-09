//! A map's Markdown: `markdown` renders one map, `catalogue` a
//! summary of several - the text powering `maps show --format md`
//! and `maps list --format md`.

use std::fmt::Write as _;

use crate::core::{Actor, EventId, Kind, Map, Node, Schema};
use crate::store::ids;

/// What every rendered map opens with, so a reader who lands on the
/// file the way they'd land on a README knows not to hand-edit it.
const PREAMBLE: &str = "Folded from the percept log for this project and rerendered on every \
    write. Change it with `percept maps`, not by hand.";

/// What the decisions map adds to the preamble: how the list below
/// reads and where the detail it leaves out still lives.
const DECISIONS_GUIDE: &str = "A `## contents` list, then every question at `##` in the order it \
    was raised - its decision, what that decision replaced (`was`), and the alternatives \
    weighed against it. Evidence and full detail: `percept maps show decisions --around \
    'question:<name>'`. What changed lately: `percept maps show decisions --since 1d`.";

/// What the tasks map adds to the preamble.
const TASKS_GUIDE: &str = "A `## contents` list, then every open task at `##` in the order it \
    was raised, each with why it matters and what it waits on. Done and dropped tasks follow \
    under `done`, each with its outcome. A task's own history: `percept maps show tasks \
    --around 'task:<name>'`. What changed lately: `percept maps show tasks --since 1d`.";

/// `map` as Markdown: a heading and the preamble, then the map's body.
/// The decisions map renders as a `## contents` list and then one `##`
/// per question, first-seen order - see `push_decisions`; the tasks map
/// the same way, then the done ones - see `push_tasks`. Every other
/// schema keeps one `## <kind>` section per node kind that holds a node
/// and a `## edges` section - see `push_by_kind`. Empty for a map with
/// no nodes, past the preamble.
pub fn markdown(map: &Map) -> String {
    let schema = map.schema();
    let mut out = format!("# {}\n\n{PREAMBLE}", schema.name);
    let guide = match schema.name.as_str() {
        "decisions" => Some(DECISIONS_GUIDE),
        "tasks" => Some(TASKS_GUIDE),
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

    match schema.name.as_str() {
        "decisions" => push_decisions(&mut out, map),
        "tasks" => push_tasks(&mut out, map),
        _ => push_by_kind(&mut out, map),
    }

    out
}

/// `maps list --format md`: one `##` section per map, in the order the
/// caller folded them. Each names the map's purpose and size, lists its
/// node and edge kinds with the gloss each carries on its `Schema`, and
/// shows one real node line and one real edge line so a reader sees the
/// shape the JSONL takes and how an edge names its ends (`kind:name`).
pub fn catalogue(maps: &[Map]) -> String {
    let mut out = String::from(
        "# maps\n\nEvery map percept knows: what it makes cheap, how big it is, \
         its node and edge kinds, and one line from it.\n",
    );
    for map in maps {
        let schema = map.schema();
        let _ = write!(
            out,
            "\n## {}\n\n{}\n\n{} nodes, {} edges.\n",
            schema.name,
            schema.purpose,
            map.nodes().len(),
            map.edges().len()
        );
        push_kind_glosses(&mut out, "Node kinds", &schema.node_kinds);
        push_kind_glosses(&mut out, "Edge kinds", &schema.edge_kinds);
        push_example(&mut out, map);
    }
    out
}

fn push_kind_glosses(out: &mut String, heading: &str, kinds: &[Kind]) {
    let _ = write!(out, "\n{heading}:\n");
    for kind in kinds {
        let _ = writeln!(out, "- {} - {}", kind.label(), kind.gloss);
    }
}

/// The map's first node line and first edge line, verbatim JSONL, under
/// an `Example` heading - or a note when the map holds none yet.
fn push_example(out: &mut String, map: &Map) {
    let node = map.nodes().first();
    let edge = map.edges().first();
    if node.is_none() && edge.is_none() {
        out.push_str("\nExample: nothing recorded here yet.\n");
        return;
    }
    out.push_str("\nExample node and edge:\n\n");
    if let Some(node) = node {
        let _ = writeln!(out, "    {}", super::map::encode_node(map, node));
    }
    if let Some(edge) = edge {
        let _ = writeln!(out, "    {}", super::map::encode_edge(map, edge));
    }
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
            push_node(out, map, node);
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
fn ordered_kinds(schema: &Schema) -> Vec<&str> {
    let mut kinds: Vec<&str> = schema.headline_kinds.iter().map(String::as_str).collect();
    for kind in schema.node_kind_names() {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
}

/// One node's bullet - its name and properties, the kind being the
/// section's - then, on its own indented line, the sources it cites,
/// when it cites any.
fn push_node(out: &mut String, map: &Map, node: &Node) {
    let _ = writeln!(out, "- {}{}", marked_name(map, node), node.properties_line());
    if !node.sources.is_empty() {
        let _ = writeln!(out, "  sources: {}", ids(&node.sources).join(", "));
    }
}

/// The decisions map's body: a `## contents` list, then one `##` per
/// question - and per decision that resolves none - in the order it was
/// raised, which never changes, so nothing a reader has seen moves.
/// Under a question: its decision, or `- open`, then the options
/// weighed against it. Under a decision: its properties one per line,
/// the prompt it cites, and one `was` line per decision it superseded,
/// nearest first.
fn push_decisions(out: &mut String, map: &Map) {
    // The headlines are the questions and the standalone decisions; a
    // decision that settles a question is shown under it, not on its own.
    let headlines: Vec<&Node> = map
        .headlines()
        .filter(|node| node.kind == "question" || !map.settles(node.id))
        .collect();
    if headlines.is_empty() {
        let _ = writeln!(
            out,
            "\n(no question or decision yet; {} nodes of other kinds.)",
            map.nodes().len()
        );
        return;
    }

    push_contents(out, map, &headlines);
    for node in headlines {
        let _ = write!(out, "\n## {}\n\n", marked_name(map, node));
        if node.kind == "question" {
            push_question_body(out, map, node);
        } else {
            // A standalone decision's `##` already names it; pass its own
            // prompt as `raised_by` so it prints no redundant `source`.
            push_decision(out, map, node, node.sources.first().copied());
        }
    }
}

/// The tasks map's body: a `## contents` list, then one `##` per open
/// task - one no outcome resolves - in the order it was raised, each
/// with why it matters and the tasks it waits on; then, under `done`,
/// every settled task with its outcome. A task never moves in the open
/// list; settling it is the one move, and the agreed one.
fn push_tasks(out: &mut String, map: &Map) {
    let (done, open): (Vec<&Node>, Vec<&Node>) = map
        .headlines()
        .partition(|task| !map.settled_by(task.id).is_empty());

    if open.is_empty() && done.is_empty() {
        let _ = writeln!(
            out,
            "\n(no task yet; {} nodes of other kinds.)",
            map.nodes().len()
        );
        return;
    }
    if !open.is_empty() {
        push_contents(out, map, &open);
    }
    for task in open {
        let _ = write!(out, "\n## {}\n\n", marked_name(map, task));
        push_props(out, task, "");
        for blocker in map.blocked_by(task.id) {
            let _ = writeln!(out, "waits on {}", marked_name(map, blocker));
        }
    }
    if !done.is_empty() {
        out.push_str("\n## done\n");
        for task in done {
            let _ = writeln!(out, "- {}", marked_name(map, task));
            for outcome in map.settled_by(task.id) {
                let _ = writeln!(out, "  outcome {}", marked_name(map, outcome));
                push_props(out, outcome, "  ");
            }
        }
    }
}

/// One `## contents` line per headline - its text and the date its
/// raising prompt was minted, in first-seen order - the overview a
/// reader scans before the entries.
fn push_contents(out: &mut String, map: &Map, headlines: &[&Node]) {
    out.push_str("\n## contents\n");
    for node in headlines {
        let _ = write!(out, "- {}", marked_name(map, node));
        match node.sources.first().and_then(|id| id.minted_at()) {
            Some(at) => {
                let _ = writeln!(out, " \u{b7} {}", at.date());
            }
            None => out.push_str(" \u{b7} uncited\n"),
        }
    }
}

/// A node's properties, each on its own line under `indent` - a long
/// `why` reads on a line of its own, never wrapped onto the name.
fn push_props(out: &mut String, node: &Node, indent: &str) {
    for (key, value) in &node.properties {
        let _ = writeln!(out, "{indent}{key}: {value:?}");
    }
}

/// The body under a question's `##`: the question's own properties, its
/// decision - or `- open` when none settles it - then one `- weighed`
/// bullet per alternative that lost, each with its own `why`.
fn push_question_body(out: &mut String, map: &Map, question: &Node) {
    push_props(out, question, "");
    let decisions = map.settled_by(question.id);
    if decisions.is_empty() {
        out.push_str("- open\n");
    }
    for decision in &decisions {
        push_decision(out, map, decision, question.sources.first().copied());
    }
    for option in map.weighed_for(question.id) {
        let _ = writeln!(out, "- weighed {}", marked_name(map, option));
        push_props(out, option, "  ");
    }
}

/// A `- decision` bullet, its properties one per line, the prompt it
/// cites when that is not `raised_by` - the prompt already shown for the
/// headline this sits under - and one `was` line per decision it
/// superseded, nearest first.
fn push_decision(out: &mut String, map: &Map, decision: &Node, raised_by: Option<EventId>) {
    let _ = writeln!(out, "- decision {}", marked_name(map, decision));
    push_props(out, decision, "  ");
    if let Some(source) = decision
        .sources
        .first()
        .filter(|id| Some(**id) != raised_by)
    {
        let _ = writeln!(out, "  source {}", source.as_uuid());
    }
    for was in map.predecessors(decision.id) {
        let _ = writeln!(out, "  was {}", marked_name(map, was));
    }
}

/// A node's short id and quoted name, marked `(model)` when the model
/// wrote it - `Actor::User` and `Actor::System` are unmarked. The short
/// id is the same `d41` `--around`, `--from`/`--to`, and a bare short
/// id in `revise_map`'s arguments all resolve.
fn marked_name(map: &Map, node: &Node) -> String {
    let mut label = match map.short_id(node.id) {
        Some(id) => format!("{id} {:?}", node.name),
        None => format!("{:?}", node.name),
    };
    if matches!(node.actor, Actor::Model) {
        label.push_str(" (model)");
    }
    label
}

#[cfg(test)]
mod tests;
