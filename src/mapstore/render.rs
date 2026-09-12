//! A map's Markdown: `markdown` renders one map, `catalogue` a
//! summary of several - the text powering `maps show` and `maps
//! list`.

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::core::{Actor, EdgeEnd, Map, Node, NodeId, Schema, Written};
use crate::store::ids;

/// `map` as Markdown: a heading, then the map's body. A map with any
/// headline kind renders one `##` section per headline node nobody
/// claims, the claimed ones nested under their claimant - see
/// `push_headlines`. A map with none renders one `## <kind>` section
/// per node kind that holds a node, plus a `## edges` section - see
/// `push_by_kind`. Empty for a map with no nodes, past the preamble.
/// A map the agent wrote alone says so once, on the heading; a map
/// with both human and agent nodes marks each agent line instead.
pub fn markdown(map: &Map) -> String {
    let schema = map.schema();
    let mark = mixed_authors(map);
    let all_agent = !mark && map.nodes().iter().any(|node| matches!(node.added().actor, Actor::Agent));
    let mut out = if all_agent {
        format!("# {} (agent)\n", schema.name)
    } else {
        format!("# {}\n", schema.name)
    };

    if map.nodes().is_empty() {
        out.push_str("\n(empty: nothing has been recorded here yet.)\n");
        return out;
    }

    if schema.headline_kinds.is_empty() {
        push_by_kind(&mut out, map, mark);
    } else {
        push_headlines(&mut out, map, mark);
    }

    out
}

/// Whether `map` holds nodes by the agent and by someone else both -
/// when a per-line `(agent)` mark tells them apart.
fn mixed_authors(map: &Map) -> bool {
    let agent = |node: &Node| matches!(node.added().actor, Actor::Agent);
    map.nodes().iter().any(agent) && !map.nodes().iter().all(agent)
}

/// `maps list`: one `##` section per map, in the order the
/// caller folded them. Each names the map's purpose and size, lists its
/// node and edge kinds with the gloss each carries on its `Schema`, and
/// shows one real node line and one real edge line so a reader sees the
/// shape the JSONL takes and how an edge names its ends (`kind:name`).
/// `maps` empty - a project with no schema declared - prints the
/// header and `super::NO_SCHEMAS_HINT` alone.
pub fn catalogue(maps: &[Map]) -> String {
    if maps.is_empty() {
        return format!("# maps\n\n{}\n", super::NO_SCHEMAS_HINT);
    }

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
        if gloss.is_empty() {
            let _ = writeln!(out, "- {label}");
        } else {
            let _ = writeln!(out, "- {label} - {gloss}");
        }
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
fn push_by_kind(out: &mut String, map: &Map, mark: bool) {
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
            push_node(out, map, node, mark);
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
fn push_node(out: &mut String, map: &Map, node: &Node, mark: bool) {
    let _ = writeln!(out, "- {}{}", marked_name(map, node, mark), node.properties_line());
    push_changed(out, node, "  ");
    if !node.sources.is_empty() {
        let _ = writeln!(out, "  sources: {}", ids(&node.sources).join(", "));
    }
}

/// One `##` section per headline node nobody claims, in an order this
/// render alone gives meaning to - never the core's: by the index of
/// its `state` property in its kind's declared list, unknown or
/// missing last, then by when it was added. A kind with no states
/// sorts by `added_at` alone. Under each heading: the node's own
/// properties, then its neighbours as `push_tree` prints them, the
/// headline nodes it claims nested with theirs. A headline node left
/// unprinted - claimed in a cycle - heads a section of its own at the
/// end, so nothing a map holds goes unseen.
fn push_headlines(out: &mut String, map: &Map, mark: bool) {
    let mut headlines: Vec<&Node> = map.headlines().collect();
    headlines.sort_by_key(|node| (state_rank(map, node), node.added().at));
    if headlines.is_empty() {
        let _ = writeln!(
            out,
            "\n(no headline node yet; {} nodes of other kinds.)",
            map.nodes().len()
        );
        return;
    }

    let roots: Vec<&Node> = headlines
        .iter()
        .copied()
        .filter(|node| claimant(map, node).is_none())
        .collect();
    let mut printed: HashSet<NodeId> = HashSet::new();
    for node in roots.iter().chain(headlines.iter()) {
        if !printed.insert(node.id) {
            continue;
        }
        let _ = write!(out, "\n## {}\n\n", marked_name(map, node, mark));
        push_props(out, node, "");
        push_tree(out, map, node, "", &mut printed, mark);
    }
}

/// The headline node `node` prints under, if any: the first headline
/// of its own kind pointing at it - the decision that supersedes it -
/// else the first headline of another kind it points at - the question
/// it resolves. A node nobody claims heads a section of its own. The
/// rule names no kind, so it holds for any schema: a `blocks` chore
/// claims the one it blocks the way `supersedes` claims the old
/// decision.
fn claimant<'a>(map: &'a Map, node: &Node) -> Option<&'a Node> {
    let headline_kinds = &map.schema().headline_kinds;
    let mut same_kind = None;
    let mut other_kind = None;
    for edge_kind in &map.schema().edge_kinds {
        for from in map.linked(node.id, &edge_kind.kind, EdgeEnd::To) {
            if same_kind.is_none() && from.kind == node.kind && headline_kinds.contains(&from.kind) {
                same_kind = Some(from);
            }
        }
        for to in map.linked(node.id, &edge_kind.kind, EdgeEnd::From) {
            if other_kind.is_none() && to.kind != node.kind && headline_kinds.contains(&to.kind) {
                other_kind = Some(to);
            }
        }
    }
    same_kind.or(other_kind)
}

/// `node`'s neighbours, one line per edge, under `indent`: an outgoing
/// edge as `- <kind> <neighbour>`, an incoming one as `- <neighbour>
/// <kind>`, edge kinds in schema order, outgoing before incoming. Each
/// neighbour prints its properties indented under its line. A
/// neighbour already printed in this map is skipped, so a node appears
/// once, where it was first reached; a headline neighbour prints here
/// only when `node` is its claimant, and then its own neighbours nest
/// one level deeper.
fn push_tree(out: &mut String, map: &Map, node: &Node, indent: &str, printed: &mut HashSet<NodeId>, mark: bool) {
    let headline_kinds = &map.schema().headline_kinds;
    let deeper = format!("{indent}  ");
    for edge_kind in &map.schema().edge_kinds {
        let outgoing = map.linked(node.id, &edge_kind.kind, EdgeEnd::From);
        let incoming = map.linked(node.id, &edge_kind.kind, EdgeEnd::To);
        let ends = outgoing
            .into_iter()
            .map(|neighbour| (EdgeEnd::From, neighbour))
            .chain(incoming.into_iter().map(|neighbour| (EdgeEnd::To, neighbour)));
        for (end, neighbour) in ends {
            let headline = headline_kinds.contains(&neighbour.kind);
            if headline && claimant(map, neighbour).is_none_or(|claimant| claimant.id != node.id) {
                continue;
            }
            if !printed.insert(neighbour.id) {
                continue;
            }
            let name = marked_name(map, neighbour, mark);
            let _ = match end {
                EdgeEnd::From => writeln!(out, "{indent}- {} {name}", edge_kind.kind),
                EdgeEnd::To => writeln!(out, "{indent}- {name} {}", edge_kind.kind),
            };
            push_props(out, neighbour, &deeper);
            if headline {
                push_tree(out, map, neighbour, &deeper, printed, mark);
            }
        }
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
    if let Some(changed) = super::changed_line(node) {
        let _ = writeln!(out, "{indent}{changed}");
    }
}

/// A node's short id and quoted name, marked `(agent)` when `mark` is
/// set and the model wrote it - `Actor::Human` and `Actor::System` are
/// never marked. The short id is the same `d41` `--around`,
/// `--from`/`--to`, and a bare short id in `revise_map`'s arguments
/// all resolve.
fn marked_name(map: &Map, node: &Node, mark: bool) -> String {
    let mut label = match map.short_id(node.id) {
        Some(id) => format!("{id} {:?}", node.name),
        None => format!("{:?}", node.name),
    };
    if mark && matches!(node.added().actor, Actor::Agent) {
        label.push_str(" (agent)");
    }
    label
}

#[cfg(test)]
mod tests;
