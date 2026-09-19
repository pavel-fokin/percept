//! A map's Markdown: `markdown` renders one map, `catalogue` a
//! summary of several - the text powering `maps show` and `maps
//! list`.

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::core::{Actor, EdgeEnd, Map, Node, NodeId, Written};
use crate::mapstore::outline;

/// `map` as Markdown: a heading, then one `##` section per node
/// nobody claims, the claimed ones nested under their claimant - see
/// `push_sections`. Empty for a map with no nodes, past the preamble.
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

    push_sections(&mut out, map, mark);
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

/// One `##` section per node nobody claims, in an order this render
/// alone gives meaning to - never the core's: by its kind's position
/// among the schema's node kinds, then by the index of its `state`
/// property in its kind's declared list, unknown or missing last, then
/// by when it was added. A kind with no states sorts by `added_at`
/// alone within its kind. Under each heading: the node's own
/// properties, then its neighbours as `push_tree` prints them, the
/// nodes it claims nested with theirs. A node left unprinted - claimed
/// in a cycle - heads a section of its own at the end, so nothing a
/// map holds goes unseen.
fn push_sections(out: &mut String, map: &Map, mark: bool) {
    let mut all: Vec<&Node> = map.nodes().iter().collect();
    all.sort_by_key(|node| (outline::rank(map, node), state_rank(map, node), node.added().at));
    let roots: Vec<&Node> = all
        .iter()
        .copied()
        .filter(|node| outline::claimant(map, node).is_none())
        .collect();
    let mut printed: HashSet<NodeId> = HashSet::new();
    for node in roots.iter().chain(all.iter()) {
        if !printed.insert(node.id) {
            continue;
        }
        let _ = write!(out, "\n## {}\n\n", marked_name(map, node, mark));
        push_props(out, node, "");
        push_tree(out, map, node, "", &mut printed, mark);
    }
}

/// `node`'s neighbours, one line per edge, under `indent`: an outgoing
/// edge as `- <kind> <neighbour>`, an incoming one as `- <neighbour>
/// <kind>`, edge kinds in schema order, outgoing before incoming. Each
/// neighbour prints its properties indented under its line. A
/// neighbour already printed in this map is skipped, so a node appears
/// once, where it was first reached; a neighbour nests here, with its
/// own neighbours one level deeper, only when `node` is its claimant.
/// One that nests elsewhere is named on its line and nothing more, so
/// a concept still lists every question about it when they nest under
/// the decisions they doubt - unless it nests inside this section, or
/// this section inside it, where a reader meets it anyway.
fn push_tree(out: &mut String, map: &Map, node: &Node, indent: &str, printed: &mut HashSet<NodeId>, mark: bool) {
    let deeper = format!("{indent}  ");
    for edge_kind in &map.schema().edge_kinds {
        let outgoing = map.linked(node.id, &edge_kind.kind, EdgeEnd::From);
        let incoming = map.linked(node.id, &edge_kind.kind, EdgeEnd::To);
        let ends = outgoing
            .into_iter()
            .map(|neighbour| (EdgeEnd::From, neighbour))
            .chain(incoming.into_iter().map(|neighbour| (EdgeEnd::To, neighbour)));
        for (end, neighbour) in ends {
            let nests_here = outline::claimant(map, neighbour).is_some_and(|claimant| claimant.id == node.id);
            if !nests_here {
                if nests_within(map, neighbour, node) || nests_within(map, node, neighbour) {
                    continue;
                }
                push_edge_line(out, map, indent, end, &edge_kind.kind, neighbour, mark);
                continue;
            }
            if !printed.insert(neighbour.id) {
                continue;
            }
            push_edge_line(out, map, indent, end, &edge_kind.kind, neighbour, mark);
            push_props(out, neighbour, &deeper);
            push_tree(out, map, neighbour, &deeper, printed, mark);
        }
    }
}

/// Whether `inner` prints somewhere inside `outer`'s section: `outer`
/// is on the chain of claimants above it. A chain that cycles ends
/// where it repeats.
fn nests_within(map: &Map, inner: &Node, outer: &Node) -> bool {
    let mut seen = HashSet::new();
    let mut node = inner;
    while let Some(above) = outline::claimant(map, node) {
        if above.id == outer.id {
            return true;
        }
        if !seen.insert(above.id) {
            return false;
        }
        node = above;
    }
    false
}

/// One edge line under a node: outgoing as `- <kind> <neighbour>`,
/// incoming as `- <neighbour> <kind>`.
fn push_edge_line(
    out: &mut String,
    map: &Map,
    indent: &str,
    end: EdgeEnd,
    edge_kind: &str,
    neighbour: &Node,
    mark: bool,
) {
    let name = marked_name(map, neighbour, mark);
    let _ = match end {
        EdgeEnd::From => writeln!(out, "{indent}- {edge_kind} {name}"),
        EdgeEnd::To => writeln!(out, "{indent}- {name} {edge_kind}"),
    };
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

/// One `changed by <actor>` line under `indent`, printed only when
/// `node`'s last change is not its addition.
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
