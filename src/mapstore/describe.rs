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

    out.push_str("\nnode kinds\n");
    push_node_kinds(&mut out, schema);

    if !schema.edge_kinds.is_empty() {
        out.push_str("\nrelations\n");
        for edge in &schema.edge_kinds {
            let _ = writeln!(
                out,
                "  {} --{}--> {}",
                edge.from.join(" | "),
                edge.kind,
                edge.to.join(" | ")
            );
        }
    }

    out.push_str("\nrecord\n");
    push_record(&mut out, schema);

    out.push_str("\nexample\n");
    push_example(&mut out, schema);

    if let Some(kind) = schema.node_kinds.iter().find(|kind| !kind.states.is_empty()) {
        out.push_str("\nexample: change\n");
        push_change_example(&mut out, kind);
    }

    out
}

/// One aligned line per node kind: its name, its gloss, and - when it
/// carries either - `requires ...` and `state ... | ...`, joined by ` ·
/// `. Names are padded to the widest kind's, glosses to the widest
/// gloss among the kinds that carry a suffix, so the suffix column
/// lines up.
fn push_node_kinds(out: &mut String, schema: &Schema) {
    let name_width = schema
        .node_kinds
        .iter()
        .map(|kind| kind.kind.chars().count())
        .max()
        .unwrap_or(0);
    let suffixes: Vec<String> = schema.node_kinds.iter().map(kind_suffix).collect();
    let gloss_width = schema
        .node_kinds
        .iter()
        .zip(&suffixes)
        .filter(|(_, suffix)| !suffix.is_empty())
        .map(|(kind, _)| kind.gloss.chars().count())
        .max()
        .unwrap_or(0);

    for (kind, suffix) in schema.node_kinds.iter().zip(&suffixes) {
        let line = if suffix.is_empty() {
            format!("  {:<name_width$}   {}", kind.kind, kind.gloss)
        } else {
            format!(
                "  {:<name_width$}   {:<gloss_width$}   {}",
                kind.kind, kind.gloss, suffix
            )
        };
        let _ = writeln!(out, "{}", line.trim_end());
    }
}

/// `requires <prop> <prop>`, ` · state <a> | <b>`, both, or neither.
fn kind_suffix(kind: &NodeKind) -> String {
    let mut suffix = String::new();
    if !kind.requires.is_empty() {
        let _ = write!(suffix, "requires {}", kind.requires.join(" "));
    }
    if !kind.states.is_empty() {
        if !suffix.is_empty() {
            suffix.push_str(" · ");
        }
        let _ = write!(suffix, "state {}", kind.states.join(" | "));
    }
    suffix
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
  property, name \"...\" renames it, and why \"...\" is the change's own reason,
  not a property. A node is refused without its required properties; a node
  the user last changed takes only state from an agent.
";

/// One node per node kind, in schema order, its name always `"..."`,
/// each required property under it as `<prop> "..."`, its first
/// declared state when it has any, and one indented edge line per kind
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
        for property in &kind.requires {
            let _ = writeln!(out, "    {property} \"...\"");
        }
        if let Some(state) = kind.states.first() {
            let _ = writeln!(out, "    state \"{state}\"");
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

/// `kind`'s short id at its first minted number, its `state` set to its
/// second declared value, or its first when it has only one, changed
/// with a `why`.
fn push_change_example(out: &mut String, kind: &NodeKind) {
    let state = kind.states.get(1).unwrap_or(&kind.states[0]);
    let _ = writeln!(out, "  {}1", kind.prefix);
    let _ = writeln!(out, "    state \"{state}\"");
    out.push_str("    why \"what happened\"\n");
}

#[cfg(test)]
mod tests;
