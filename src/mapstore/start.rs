//! `percept start`'s text, and the session-start hook's: a read-only
//! render of a project's cognitive state - what each map holds, what
//! moved since the caller's last look, what it cites that the working
//! tree no longer holds, and the command that opens each. Beside
//! `markdown`, `catalogue`, and `describe`: one more external form of
//! a map, folded by the caller and rendered here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::blocks::{capped_lines, changed_line, line_id, LIMIT};
use crate::core::{cited_label, Event, EventId, Map, Node, Payload, Written};
use crate::shared::Timestamp;
use crate::workspace;

/// `maps`, `events`, `root`, and `checkout` as the three blocks
/// `start` prints: State, what each map holds; Attention, what moved
/// since `since` and what it cites that no longer matches the tree;
/// Next, the command that opens each. Empty when no map holds any node
/// at all - a stranger's first run. `since` is the caller's cut: the
/// hook passes its own client's last session, so a review-page open
/// never hides a client's gains from it, and `percept start` from the
/// shell passes the last look by anyone, the running session's own
/// start included. `checkout` is where cited files are read.
pub fn start(maps: &[Map], events: &[Event], root: &Path, checkout: &Path, since: Option<Timestamp>) -> String {
    let project = project_name(root);

    if maps.iter().all(|map| map.nodes().is_empty()) {
        let mut lines = vec![format!("percept \u{b7} {project}\nnothing recorded yet\n\nNext")];
        lines.extend(pad_rows(&[how_to_record()]));
        return lines.join("\n");
    }

    let names: Vec<&str> = maps.iter().map(|map| map.schema().name.as_str()).collect();
    let mut sections = vec![format!(
        "percept \u{b7} {project}\nkeeps what this project settled: {}",
        names.join(", ")
    )];

    let moved: Vec<Vec<&Node>> = maps
        .iter()
        .map(|map| since.map_or_else(Vec::new, |since| gained(map, since)))
        .collect();
    sections.push(state_block(maps, &moved));
    let (attention, printed) = attention_block(maps, &moved, events, checkout);
    sections.extend(attention);
    sections.push(next_block(maps, &printed));

    sections.join("\n\n")
}

/// `root`'s last path component, the name a reader knows the project
/// by - falling back to the whole path on the rare root with none.
fn project_name(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

/// The headline nodes of `map` whose last change happened at or after
/// `since` - a node's last change is compared directly, not
/// `Map::since`, which would also surface an older node a fresh edge
/// only touched.
fn gained(map: &Map, since: Timestamp) -> Vec<&Node> {
    map.headlines().filter(|node| node.changed().at >= since).collect()
}

/// One Attention row: the node's short id and the map it is on, which
/// `next_block` turns into a `read around` line, and the line printed.
struct Row {
    id: String,
    map: String,
    line: String,
}

/// What every current headline node cites that no longer matches the
/// working tree - one Attention row per node with at least one stale
/// citation, `<id> cites <label> changed, <label> gone`, `findings` each `"<label> changed"` or `"<label>
/// gone"`. This module's Attention block builds its lines from this.
///
/// Builds two indexes over `events` once, both over `file.cited`
/// events only - id to event, and causation id to the events it
/// caused - so no node's check re-reads the log: `id_to_event`
/// resolves a node's `sources` entries, `later_citations` walks a
/// citation forward to the newest re-citation of the same file before
/// it is checked against the tree. `cache` memoises each cited path's
/// normalised tree text - `None` for one that is gone - for the rest
/// of this call, so a path cited by more than one node is read once.
fn citation_rows(maps: &[Map], events: &[Event], checkout: &Path) -> Vec<Row> {
    let file_cited: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event.payload(), Payload::FileCited { .. }))
        .collect();
    let id_to_event: HashMap<EventId, &Event> =
        file_cited.iter().map(|event| (event.id(), *event)).collect();
    let mut later_citations: HashMap<EventId, Vec<&Event>> = HashMap::new();
    for event in &file_cited {
        if let Some(cause) = event.causation_id() {
            later_citations.entry(cause).or_default().push(event);
        }
    }

    let mut cache: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut rows = Vec::new();
    for map in maps {
        for node in map.headlines() {
            let findings = node_changes(node, &id_to_event, &later_citations, checkout, &mut cache);
            if !findings.is_empty() {
                let id = line_id(map, node);
                let line = format!("{id} cites {}", findings.join(", "));
                rows.push(Row { id, map: map.schema().name.clone(), line });
            }
        }
    }
    rows
}

/// `node`'s own `changed`/`gone` findings, one per source that names a
/// `file.cited` event, checked at its newest re-citation.
fn node_changes(
    node: &Node,
    id_to_event: &HashMap<EventId, &Event>,
    later_citations: &HashMap<EventId, Vec<&Event>>,
    checkout: &Path,
    cache: &mut HashMap<PathBuf, Option<String>>,
) -> Vec<String> {
    node.sources
        .iter()
        .filter_map(|source_id| {
            let event = id_to_event.get(source_id)?;
            let newest = newest_citation(event, later_citations);
            let Payload::FileCited { path, lines, excerpt } = newest.payload() else {
                return None;
            };
            citation_status(cache, checkout, path, excerpt)
                .map(|status| format!("{} {status}", cited_label(path, *lines)))
        })
        .collect()
}

/// Follows `event` forward through `later_citations`, each hop the
/// latest re-citation of the same file caused by the one before it -
/// a later citation of a different path is not a re-citation of this
/// one, so it is ignored. A visited set stops a causation cycle a
/// hand-edited log could hold from spinning forever.
fn newest_citation<'a>(
    event: &'a Event,
    later_citations: &HashMap<EventId, Vec<&'a Event>>,
) -> &'a Event {
    let Payload::FileCited { path, .. } = event.payload() else {
        return event;
    };
    let mut current = event;
    let mut visited = HashSet::from([event.id()]);
    while let Some(next) = later_citations
        .get(&current.id())
        .into_iter()
        .flatten()
        .filter(|candidate| {
            matches!(candidate.payload(), Payload::FileCited { path: p, .. } if p == path)
        })
        .filter(|candidate| visited.insert(candidate.id()))
        .max_by_key(|candidate| candidate.created_at())
    {
        current = next;
    }
    current
}

/// `gone` when `path` under `checkout` is missing or binary; `changed`
/// when it no longer contains `excerpt` as a substring, or `excerpt`
/// normalises to nothing to compare against; `None` when it still
/// reads. Both sides go through `normalize`; the tree's side is read
/// through `cache`, so a path more than one citation names is read and
/// normalised once.
fn citation_status(
    cache: &mut HashMap<PathBuf, Option<String>>,
    checkout: &Path,
    path: &Path,
    excerpt: &str,
) -> Option<&'static str> {
    let excerpt = normalize(excerpt);
    if excerpt.is_empty() {
        return Some("changed");
    }
    let text = cache
        .entry(path.to_path_buf())
        .or_insert_with(|| workspace::read_text_lossy(&checkout.join(path)).ok().map(|t| normalize(&t)));
    match text {
        Some(text) if text.contains(&excerpt) => None,
        _ => Some(if text.is_some() { "changed" } else { "gone" }),
    }
}

/// Each line's trailing whitespace stripped, then leading and trailing
/// blank lines trimmed - the one normalisation both sides of a
/// `changed since recorded` comparison go through, so a citation whose
/// stored excerpt padded its range with context still matches.
fn normalize(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(|line| line.trim_end()).collect();
    let start = lines.iter().position(|line| !line.is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|line| !line.is_empty()).map_or(start, |i| i + 1);
    lines[start..end].join("\n")
}

/// A block's rows, indented and padded so every right-hand value
/// starts at the same column - the widest left-hand text in the block
/// plus three spaces.
fn pad_rows(rows: &[(String, String)]) -> Vec<String> {
    let width = rows.iter().map(|(left, _)| left.chars().count()).max().unwrap_or(0) + 3;
    rows.iter()
        .map(|(left, right)| format!("  {left:<width$}{right}"))
        .collect()
}

/// `<n> <state>` for every declared state of every headline kind of
/// `map`, in declared order, skipping states no headline node is in -
/// `1 open   2 done` for a kind with those states. Every state is
/// counted, since a schema lists them as a set and no position means
/// "initial".
fn state_counts(map: &Map) -> Vec<String> {
    let schema = map.schema();
    schema
        .node_kinds
        .iter()
        .filter(|kind| schema.headline_kinds.contains(&kind.kind))
        .flat_map(|kind| {
            kind.states.iter().filter_map(|state| {
                let count = map
                    .headlines()
                    .filter(|node| node.kind == kind.kind && node.properties.get("state") == Some(state))
                    .count();
                (count > 0).then(|| format!("{count} {state}"))
            })
        })
        .collect()
}

/// The State block: one line per map in fold order - its headline
/// count, `+N since last session` when that map's `moved` list is not
/// empty, then any state counts `state_counts` finds.
fn state_block(maps: &[Map], moved: &[Vec<&Node>]) -> String {
    let rows: Vec<(String, String)> = maps
        .iter()
        .zip(moved)
        .map(|(map, moved)| {
            let mut parts = vec![map.headlines().count().to_string()];
            if !moved.is_empty() {
                parts.push(format!("+{} since last session", moved.len()));
            }
            parts.extend(state_counts(map));
            (map.schema().name.clone(), parts.join("   "))
        })
        .collect();

    let mut lines = vec!["State".to_string()];
    lines.extend(pad_rows(&rows));
    lines.join("\n")
}

/// The Attention block: each map's `moved` nodes, with `added` or the
/// node's last change after a ` · `, then the stale citations, each
/// list capped at `LIMIT` lines. Rows are not padded into columns: a
/// node's name sets no column width. Returns the block with the id and
/// map of every node it printed, in order, each once - the `+N more`
/// line names none - so `next_block` offers a `read around` for each.
/// `None` when both lists are empty, so `start` omits the block rather
/// than printing an empty one.
fn attention_block(
    maps: &[Map],
    moved: &[Vec<&Node>],
    events: &[Event],
    checkout: &Path,
) -> (Option<String>, Vec<(String, String)>) {
    let moved: Vec<Row> = maps
        .iter()
        .zip(moved)
        .flat_map(|(map, nodes)| {
            nodes.iter().map(move |node| {
                let id = line_id(map, node);
                let change = changed_line(node).unwrap_or_else(|| "added".to_string());
                let line = format!("{id} {} {:?} \u{b7} {change}", node.kind, node.name);
                Row { id, map: map.schema().name.clone(), line }
            })
        })
        .collect();
    let cited = citation_rows(maps, events, checkout);

    if moved.is_empty() && cited.is_empty() {
        return (None, Vec::new());
    }

    let mut lines = vec!["Attention".to_string()];
    let mut printed: Vec<(String, String)> = Vec::new();
    for rows in [moved, cited] {
        for row in rows.iter().take(LIMIT) {
            if !printed.iter().any(|(id, _)| *id == row.id) {
                printed.push((row.id.clone(), row.map.clone()));
            }
        }
        let capped = capped_lines(rows.into_iter().map(|row| row.line).collect());
        lines.extend(capped.into_iter().map(|line| format!("  {line}")));
    }
    (Some(lines.join("\n")), printed)
}

/// The Next block: `read <map>` for every map with a headline node,
/// `read around <id>` for every node Attention printed, then the two
/// fixed pointers every render carries.
fn next_block(maps: &[Map], printed: &[(String, String)]) -> String {
    let mut rows: Vec<(String, String)> = maps
        .iter()
        .filter(|map| map.headlines().next().is_some())
        .map(|map| {
            let name = &map.schema().name;
            (format!("read {name}"), format!("percept maps show {name}"))
        })
        .collect();
    for (id, map_name) in printed {
        rows.push((
            format!("read around {id}"),
            format!("percept maps show {map_name} --around {id}"),
        ));
    }
    rows.push((
        "search the log".to_string(),
        "percept events search --contains <text>".to_string(),
    ));
    rows.push(how_to_record());

    let mut lines = vec!["Next".to_string()];
    lines.extend(pad_rows(&rows));
    lines.join("\n")
}

/// The one Next row every render carries, the empty state included.
fn how_to_record() -> (String, String) {
    ("how to record".to_string(), "percept maps describe <map>".to_string())
}

#[cfg(test)]
mod tests;
