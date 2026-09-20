//! A map's Markdown: `markdown` renders one map, `catalogue` a
//! summary of several - the text powering `maps show` and `maps
//! list`.

use std::fmt::Write as _;

use crate::core::{Actor, EdgeKind, Map, Node, NodeKind, Written};
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

/// A map's overview for a prompt: the nodes that head it, one line
/// each, as `Map`'s `Display` formats a node, without properties - a
/// reader deciding whether to open the map with `read_map` doesn't
/// need them yet.
pub fn overview(map: &Map) -> String {
    let lines: Vec<String> = outline::roots(map).into_iter().map(|node| format!("- {node}")).collect();
    format!(
        "The nodes that head it follow; read_map opens the rest, whole or around one node.\n{}",
        lines.join("\n")
    )
}

/// Whether `map` holds nodes by the agent and by someone else both -
/// when a per-line `(agent)` mark tells them apart.
fn mixed_authors(map: &Map) -> bool {
    let agent = |node: &Node| matches!(node.added().actor, Actor::Agent);
    map.nodes().iter().any(agent) && !map.nodes().iter().all(agent)
}

/// `maps list`: one `##` section per map, in the order the
/// caller folded them. Each names the map's purpose and size, lists its
/// node and edge kinds, and shows one real node line and one real edge
/// line so a reader sees the shape the JSONL takes and how an edge
/// names its ends (`kind:name`). `maps` empty - a project with no
/// schema declared - prints the header and `super::NO_SCHEMAS_HINT`
/// alone.
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
        push_kind_labels(&mut out, "Node kinds", schema.node_kinds.iter().map(NodeKind::label));
        push_kind_labels(&mut out, "Edge kinds", schema.edge_kinds.iter().map(EdgeKind::label));
        push_example(&mut out, map);
    }
    out
}

fn push_kind_labels(out: &mut String, heading: &str, labels: impl Iterator<Item = String>) {
    let _ = write!(out, "\n{heading}:\n");
    for label in labels {
        let _ = writeln!(out, "- {label}");
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

/// One `##` section per root, in an order this render alone gives
/// meaning to - never the core's: by the index of its kind's closed
/// list value - `state` on a `task`, say - in that list's declared
/// order, unknown or missing last, then by when it was added. Under
/// each heading: the node's own properties, then what hangs under it,
/// as `push_tree` prints it. A forest, so every node is reached from
/// exactly one root and nothing a map holds goes unseen.
fn push_sections(out: &mut String, map: &Map, mark: bool) {
    let mut roots = outline::roots(map);
    roots.sort_by_cached_key(|node| (closed_list_rank(map, node), node.added().at));
    for node in roots {
        let _ = write!(out, "\n## {}\n\n", marked_name(map, node, mark));
        push_props(out, map, node, "");
        push_tree(out, map, node, "", mark);
    }
}

/// What hangs under `node`, one line per child as `- <kind> <child>`,
/// each child's properties indented under its line and its own
/// children a level deeper. A forest, so the walk ends and no node is
/// met twice.
fn push_tree(out: &mut String, map: &Map, node: &Node, indent: &str, mark: bool) {
    let deeper = format!("{indent}  ");
    for (kind, child) in map.children(node.id) {
        let _ = writeln!(out, "{indent}- {kind} {}", marked_name(map, child, mark));
        push_props(out, map, child, &deeper);
        push_tree(out, map, child, &deeper, mark);
    }
}

/// `node`'s position among its kind's closed list - unknown, missing,
/// or a kind with no closed list sort last, so listing order says
/// nothing the core does not already know from the property itself.
fn closed_list_rank(map: &Map, node: &Node) -> usize {
    let Some(kind) = map.schema().node_kind(&node.kind) else {
        return usize::MAX;
    };
    let Some((property, values)) = kind.closed_list() else {
        return usize::MAX;
    };
    let Some(value) = map.property(node, property) else {
        return usize::MAX;
    };
    values.iter().position(|v| v == value).unwrap_or(usize::MAX)
}

/// A node's properties, each on its own line under `indent` - a long
/// `why` reads on a line of its own, never wrapped onto the name - then
/// one `changed by` line, when the node's last change is not its
/// addition. Written values and a closed list's default both, as
/// `Map::properties` gives them - a reader here meets the same values
/// a fresh node of this kind would start from.
fn push_props(out: &mut String, map: &Map, node: &Node, indent: &str) {
    for (key, value) in map.properties(node) {
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
