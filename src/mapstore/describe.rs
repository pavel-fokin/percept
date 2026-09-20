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

    if !schema.rules.is_empty() {
        out.push_str("\nrules\n");
        push_rules(&mut out, schema);
    }

    out.push_str("\nrecord\n");
    push_record(&mut out, schema);

    out.push_str("\nexample\n");
    push_example(&mut out, schema);

    if let Some(kind) = schema.node_kinds.iter().find(|kind| kind.closed_list().is_some()) {
        out.push_str("\nexample: change\n");
        push_change_example(&mut out, kind);
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

/// Every moment's rules, one line per entry: the moment name before its
/// first line, later lines of the same moment aligned under it. Moment
/// names are padded to the widest, so every line's rule starts at the
/// same column.
fn push_rules(out: &mut String, schema: &Schema) {
    let moments: Vec<(&str, &[String])> = schema.rules.iter().collect();
    let width = moments.iter().map(|(moment, _)| moment.len()).max().unwrap_or(0);
    let indent = " ".repeat(width);
    for (moment, lines) in moments {
        for (index, line) in lines.iter().enumerate() {
            let prefix = if index == 0 { moment } else { indent.as_str() };
            let _ = writeln!(out, "  {prefix:<width$}   {line}");
        }
    }
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
  property and name \"...\" renames it. A node is refused without its required
  properties; a node the user last changed takes only state from an agent.
  This document adds and changes; it never removes. percept maps remove-node
  and remove-edge do that, each taking the same --actor and --source, and
  remove-node drops the edges that touch the node it takes.
";

/// One node per node kind, in schema order, its name always `"..."`,
/// each declared property under it as `<prop> "..."` - a closed list's
/// first value in place of `...` - and one indented edge line per kind
/// already listed above it that an edge from this kind may reach - the
/// first such edge kind, so a node is not shown both supporting and
/// contradicting the same neighbour. The last kind carries the `cites`
/// line, being the one a claim resting on code would be written as;
/// its path is a placeholder, so the example shows the shape rather
/// than running as it stands.
fn push_example(out: &mut String, schema: &Schema) {
    let mut listed: Vec<&str> = Vec::new();
    let last = schema.node_kinds.len().saturating_sub(1);
    for (index, kind) in schema.node_kinds.iter().enumerate() {
        let _ = writeln!(out, "  {} \"...\"", kind.kind);
        for (property, values) in &kind.properties {
            let value = values.first().map_or("...", String::as_str);
            let _ = writeln!(out, "    {property} \"{value}\"");
        }
        let mut reached: Vec<&str> = Vec::new();
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
        if index == last {
            out.push_str("    cites src/path.rs:10-20\n");
        }
        listed.push(&kind.kind);
    }
}

/// `kind`'s short id at its first minted number, its closed-list
/// property set to its second declared value, or its first when it has
/// only one.
fn push_change_example(out: &mut String, kind: &NodeKind) {
    let (property, values) = kind.closed_list().expect("caller found a kind with a closed list");
    let value = values.get(1).unwrap_or(&values[0]);
    let _ = writeln!(out, "  {}1", kind.prefix);
    let _ = writeln!(out, "    {property} \"{value}\"");
}

#[cfg(test)]
mod tests;
