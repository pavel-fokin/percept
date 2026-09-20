//! One map's capabilities, printed from its `Schema` alone: what it can
//! hold and how to write to it, with no log and no fold. `maps describe`
//! prints this on an empty map, and a project's own schema - a
//! glossary, say - gets the same shape.

use std::fmt::Write as _;

use crate::core::{NodeKind, Schema};

/// `schema` as text: its purpose, its node and edge kinds, how to
/// record to it, and two worked examples - one that adds to the map,
/// one that changes a node already in it, present only when some node
/// kind declares states.
pub fn describe(schema: &Schema) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", schema.name);
    let _ = writeln!(out, "{}", schema.purpose);

    out.push_str("\nnode kinds, coarsest first\n");
    push_node_kinds(&mut out, schema);

    if !schema.edge_kinds.is_empty() {
        out.push_str("\nrelations, each from the node above to the one under it\n");
        push_edge_kinds(&mut out, schema);
    }

    out.push_str("\nrecord\n");
    push_record(&mut out, schema);

    out.push_str("\nexample\n");
    push_example(&mut out, schema);

    if let Some((kind, closed)) = schema
        .node_kinds
        .iter()
        .find_map(|kind| kind.closed_list().map(|closed| (kind, closed)))
    {
        out.push_str("\nexample: change\n");
        push_change_example(&mut out, kind, closed);
    }

    out
}

/// One aligned line per node kind: its name, then - when it declares
/// any - its properties, joined by ` · `, each a closed list's values
/// after its name. Names are padded to the widest kind's, so the
/// suffix column lines up.
fn push_node_kinds(out: &mut String, schema: &Schema) {
    let name_width = schema
        .node_kinds
        .iter()
        .map(|kind| kind.kind.chars().count())
        .max()
        .unwrap_or(0);
    for kind in &schema.node_kinds {
        let line = format!("  {:<name_width$}   {}", kind.kind, kind_suffix(kind));
        let _ = writeln!(out, "{}", line.trim_end());
    }
}

/// One line per edge kind, `<from> --<kind>--> <to>`.
fn push_edge_kinds(out: &mut String, schema: &Schema) {
    for edge in &schema.edge_kinds {
        let _ = writeln!(out, "  {} --{}--> {}", edge.from.join(" | "), edge.kind, edge.to.join(" | "));
    }
}

/// Every property a node kind declares, each `<name>` or `<name> <a> |
/// <b>` for a closed list, joined by ` · `.
fn kind_suffix(kind: &NodeKind) -> String {
    kind.properties
        .iter()
        .map(|(name, values)| {
            if values.is_empty() {
                name.clone()
            } else {
                format!("{name} {}", values.join(" | "))
            }
        })
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

fn push_record(out: &mut String, schema: &Schema) {
    let _ = writeln!(
        out,
        "  percept maps record {} --actor <human|agent> --source <event id> <<'EOF'",
        schema.name
    );
    out.push_str("  <document>\n  EOF\n");
    out.push_str(GRAMMAR);
}

/// The record document's grammar, the same for every map.
const GRAMMAR: &str = "
  A line at the margin is a node, kind \"name\". An indented line under it is a
  property, why \"...\"; an edge to a short id or to the latest node of that kind
  above it, resolves question; or cites src/path.rs:10-20, which records the
  text as seen and adds it to the node's sources. A claim that rests on code
  cites it, so a later session is told when that code has changed. A margin
  line naming a short id, t4, changes that node: state \"done\" under it sets a
  property and name \"...\" renames it. A node written without its kind's
  closed-list property starts from that list's first value. A node the user
  last changed takes no change from an agent at all.
  This document adds and changes; it never removes. percept maps remove-node
  and remove-edge do that, each taking the same --actor and --source, and
  remove-node drops the edges that touch the node it takes.
";

/// One node per node kind, finest first - the reverse of the schema's
/// own coarsest-first order, because an edge runs from a node to the
/// one hanging under it, so the child must stand above the parent that
/// names it. Its name is always `"..."`,
/// each declared property under it as `<prop> "..."` - a closed list's
/// first value in place of `...` - and one indented edge line per kind
/// already listed above it that an edge from this kind may reach - the
/// first such edge kind, so a node is not shown both supporting and
/// contradicting the same neighbour. The first kind written - the
/// finest - carries the `cites` line, being the one a claim resting on
/// code would be written as; its path is a placeholder, so the
/// example shows the shape rather than running as it stands.
fn push_example(out: &mut String, schema: &Schema) {
    let mut listed: Vec<&str> = Vec::new();
    // One edge into a kind across the whole example, not one per node
    // written: the example is kept a tree, one parent per kind, so a
    // reader can follow it at a glance.
    let mut reached: Vec<&str> = Vec::new();
    for (index, kind) in schema.node_kinds.iter().rev().enumerate() {
        let _ = writeln!(out, "  {} \"...\"", kind.kind);
        for (property, values) in &kind.properties {
            let value = values.first().map_or("...", String::as_str);
            let _ = writeln!(out, "    {property} \"{value}\"");
        }
        for edge in &schema.edge_kinds {
            if !edge.from.iter().any(|from| from == kind.kind.as_str()) {
                continue;
            }
            let to = edge
                .to
                .iter()
                .find(|to| listed.contains(&to.as_str()) && !reached.contains(&to.as_str()));
            if let Some(to) = to {
                let _ = writeln!(out, "    {} {}", edge.kind, to);
                reached.push(to);
            }
        }
        if index == 0 {
            out.push_str("    cites src/path.rs:10-20\n");
        }
        listed.push(&kind.kind);
    }
}

/// `kind`'s short id at its first minted number, its closed-list
/// property set to its second declared value, or its first when it has
/// only one.
fn push_change_example(out: &mut String, kind: &NodeKind, (property, values): (&str, &[String])) {
    let value = values.get(1).unwrap_or(&values[0]);
    let _ = writeln!(out, "  {}1", kind.prefix);
    let _ = writeln!(out, "    {property} \"{value}\"");
}

#[cfg(test)]
mod tests;
