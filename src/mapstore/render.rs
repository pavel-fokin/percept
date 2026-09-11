//! A map's Markdown: `markdown` renders one map, `catalogue` a
//! summary of several - the text powering `maps show --format md`
//! and `maps list --format md`.

use std::fmt::Write as _;

use crate::core::{Actor, Dir, Map, Node, Schema};
use crate::store::ids;

/// What every rendered map opens with, so a reader who pastes it
/// somewhere knows it is a fold, not a file to hand-edit.
const PREAMBLE: &str = "Folded live from the percept log for this project, never written to a \
    file. Change it with `percept maps`, not by hand.";

/// `map` as Markdown: a heading and the preamble, then the map's body.
/// A map with any headline kind renders as a `## contents` list and one
/// `##` section per headline node, in the order `push_headlines` gives -
/// see there. A map with none renders one `## <kind>` section per node
/// kind that holds a node, plus a `## edges` section - see
/// `push_by_kind`. Empty for a map with no nodes, past the preamble.
pub fn markdown(map: &Map) -> String {
    let schema = map.schema();
    let mut out = format!("# {}\n\n{PREAMBLE} {}\n", schema.name, guide(&schema.name));

    if map.nodes().is_empty() {
        out.push_str("\n(empty: nothing has been recorded here yet.)\n");
        return out;
    }

    if schema.headline_kinds.is_empty() {
        push_by_kind(&mut out, map);
    } else {
        push_headlines(&mut out, map);
    }

    out
}

/// The line every map's intro adds to the preamble, naming no kind:
/// what the contents list gives, and the two ways to look further -
/// around a node, or since an instant.
fn guide(map: &str) -> String {
    format!(
        "A `## contents` list, then one `##` section per node it names, holding that node's \
         properties and its edges. A node's neighbourhood: `percept maps show {map} --around \
         'kind:name'`. What changed lately: `percept maps show {map} --since 1d`."
    )
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
        push_kind_glosses(
            &mut out,
            "Node kinds",
            schema.node_kinds.iter().map(|k| (k.label(), k.gloss.as_str())),
        );
        push_kind_glosses(
            &mut out,
            "Edge kinds",
            schema.edge_kinds.iter().map(|k| (k.label(), k.gloss.as_str())),
        );
        push_example(&mut out, map);
    }
    out
}

fn push_kind_glosses<'a>(
    out: &mut String,
    heading: &str,
    kinds: impl Iterator<Item = (String, &'a str)>,
) {
    let _ = write!(out, "\n{heading}:\n");
    for (label, gloss) in kinds {
        let _ = writeln!(out, "- {label} - {gloss}");
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
        let _ = writeln!(out, "    {}", super::map::encode_node(map, node, true));
    }
    if let Some(edge) = edge {
        let _ = writeln!(out, "    {}", super::map::encode_edge(map, edge, true));
    }
}

/// One `## <kind>` section per node kind that holds a node - headline
/// kinds first, then the schema's remaining kinds - then a `## edges`
/// section when the map has any. The fallback for a schema that
/// declares no headline kind at all, so a map like that still lists
/// everything somewhere.
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
    push_changed(out, node, "  ");
    if !node.sources.is_empty() {
        let _ = writeln!(out, "  sources: {}", ids(&node.sources).join(", "));
    }
}

/// A `## contents` list, then one `##` section per headline node, in an
/// order this render alone gives meaning to - never the core's: by the
/// index of its `state` property in its kind's declared list, unknown
/// or missing last, then by when it was added. A kind with no states
/// sorts by `added_at` alone. Under each heading: the node's own
/// properties, then its edges, one line per edge kind the schema
/// declares, in schema order.
fn push_headlines(out: &mut String, map: &Map) {
    let mut headlines: Vec<&Node> = map.headlines().collect();
    headlines.sort_by_key(|node| (state_rank(map, node), node.added_at));
    if headlines.is_empty() {
        let _ = writeln!(
            out,
            "\n(no headline node yet; {} nodes of other kinds.)",
            map.nodes().len()
        );
        return;
    }

    push_contents(out, map, &headlines);
    for node in &headlines {
        let _ = write!(out, "\n## {}\n\n", marked_name(map, node));
        push_props(out, node, "");
        push_edges(out, map, node);
    }
}

/// `node`'s position among its kind's declared states - unknown,
/// missing, or a kind with no states sort last, so listing order says
/// nothing the core does not already know from the property itself.
fn state_rank(map: &Map, node: &Node) -> usize {
    let Some(kind) = map.schema().node_kind(&node.kind) else {
        return usize::MAX;
    };
    let Some(value) = node.properties.get("state") else {
        return usize::MAX;
    };
    kind.states
        .iter()
        .position(|state| state == value)
        .unwrap_or(usize::MAX)
}

/// One `## contents` line per headline, in the order given: its short
/// id and name, plus `[<state>]` when it carries a `state` property.
fn push_contents(out: &mut String, map: &Map, headlines: &[&Node]) {
    out.push_str("\n## contents\n");
    for node in headlines {
        let _ = write!(out, "- {}", marked_name(map, node));
        if let Some(state) = node.properties.get("state") {
            let _ = write!(out, " [{state}]");
        }
        out.push('\n');
    }
}

/// A node's properties, each on its own line under `indent` - a long
/// `why` reads on a line of its own, never wrapped onto the name - then
/// one `changed by` line, when the node's last change is not its
/// addition.
fn push_props(out: &mut String, node: &Node, indent: &str) {
    for (key, value) in &node.properties {
        let _ = writeln!(out, "{indent}{key}: {value:?}");
    }
    push_changed(out, node, indent);
}

/// One `changed by <actor>` line under `indent`, with `: "<why>"`
/// appended when the change carried one - printed only when `node`'s
/// last change is not its addition.
fn push_changed(out: &mut String, node: &Node, indent: &str) {
    if node.changed_at == node.added_at {
        return;
    }
    let _ = write!(out, "{indent}changed by {}", node.changed_by.name());
    if let Some(why) = &node.changed_why {
        let _ = write!(out, ": {why:?}");
    }
    out.push('\n');
}

/// `node`'s edges, one line per edge kind the schema declares, in
/// schema order: an outgoing edge as `- <kind> <neighbour>`, an
/// incoming one as `- <neighbour> <kind>`, the neighbour named the way
/// `marked_name` names any node. A neighbour of a kind that is not a
/// headline also prints its own properties indented under that line -
/// the one hop an option's `why` stays from its question; a second hop,
/// like evidence under that option, is never reached from here.
fn push_edges(out: &mut String, map: &Map, node: &Node) {
    let headline_kinds = &map.schema().headline_kinds;
    for edge_kind in &map.schema().edge_kinds {
        for neighbour in map.linked(node.id, &edge_kind.name, Dir::From) {
            let _ = writeln!(out, "- {} {}", edge_kind.name, marked_name(map, neighbour));
            if !headline_kinds.contains(&neighbour.kind) {
                push_props(out, neighbour, "  ");
            }
        }
        for neighbour in map.linked(node.id, &edge_kind.name, Dir::To) {
            let _ = writeln!(out, "- {} {}", marked_name(map, neighbour), edge_kind.name);
            if !headline_kinds.contains(&neighbour.kind) {
                push_props(out, neighbour, "  ");
            }
        }
    }
}

/// A node's short id and quoted name, marked `(agent)` when the model
/// wrote it - `Actor::Human` and `Actor::System` are unmarked. The
/// short id is the same `d41` `--around`, `--from`/`--to`, and a bare
/// short id in `revise_map`'s arguments all resolve.
fn marked_name(map: &Map, node: &Node) -> String {
    let mut label = match map.short_id(node.id) {
        Some(id) => format!("{id} {:?}", node.name),
        None => format!("{:?}", node.name),
    };
    if matches!(node.actor, Actor::Agent) {
        label.push_str(" (agent)");
    }
    label
}

#[cfg(test)]
mod tests;
