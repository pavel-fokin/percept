//! The block renderers `percept hook`'s session-start block and the
//! review page's foot both print, and the shared session rule both a
//! hook and the review page fold sessions by: `latest_session_per
//! client` picks out the latest `session.started` per writer and
//! project, in whatever scope the caller folds.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::core::{Event, Map, Node, Payload, Scope, Standing};
use crate::shared::Timestamp;

/// How many lines of a gained, changed, or open list a block shows
/// before folding the rest into a trailing count.
pub(crate) const LIMIT: usize = 5;

/// Up to `LIMIT` of `lines`, with a trailing `+N more` when there were
/// more - the one truncation rule every session-start block shares.
pub(crate) fn capped_lines(mut lines: Vec<String>) -> Vec<String> {
    let total = lines.len();
    lines.truncate(LIMIT);
    if total > LIMIT {
        lines.push(format!("+{} more", total - LIMIT));
    }
    lines
}

/// The header of a capped block: `label (total)`, or `label (total,
/// showing LIMIT)` when `capped_lines` folds the rest into a count.
pub(crate) fn block_header(label: &str, total: usize) -> String {
    if total > LIMIT {
        format!("{label} ({total}, showing {LIMIT})")
    } else {
        format!("{label} ({total})")
    }
}

/// The short id `node` has on `map`, or a `kind:name` fallback for the
/// unexpected case a headline node carries none.
pub(crate) fn line_id(map: &Map, node: &Node) -> String {
    map.short_id(node.id)
        .unwrap_or_else(|| format!("{}:{}", node.kind, node.name))
}

/// What the human judged since `since`: every node, of any kind, whose
/// latest `claim.confirmed`/`claim.disputed` landed at or after `since`,
/// across every folded map in fold order. Disputed nodes first, then
/// confirmed, each group by when the judgment landed, then by short id
/// so equal times print in one order. `None` when nothing was judged
/// since, so a caller omits the block rather than printing an empty
/// one.
pub(crate) fn judged_since_block(maps: &[Map], since: Timestamp) -> Option<String> {
    let mut judged: Vec<(&Map, &Node, Standing, Timestamp)> = maps
        .iter()
        .flat_map(|map| {
            map.judged_since(since)
                .map(move |(node, standing, at)| (map, node, standing, at))
        })
        .collect();
    if judged.is_empty() {
        return None;
    }
    judged.sort_by_cached_key(|(_, node, standing, at)| {
        (*standing == Standing::Confirmed, *at, node.kind.clone(), node.seq)
    });

    let lines = judged
        .iter()
        .map(|(map, node, standing, _)| {
            let mut line = format!("{} {node} \u{b7} {standing}", line_id(map, node));
            if let Some(why) = map.dispute(node.id) {
                line.push_str(&format!(": {why:?}"));
            }
            line
        })
        .collect();

    let mut block = vec![block_header("judged since your last session", judged.len())];
    block.extend(capped_lines(lines));
    Some(block.join("\n"))
}

/// The latest `session.started` per writer and project root, among
/// `events` that fall inside `scope`. `cli::hook::last_session` looks
/// its own client up in this; `server::review::last_session` takes the
/// min across every value, with the earliest admitted event as its
/// fallback.
pub(crate) fn latest_session_per_client(
    events: &[Event],
    scope: &Scope,
) -> HashMap<(String, PathBuf), Timestamp> {
    let mut latest: HashMap<(String, PathBuf), Timestamp> = HashMap::new();
    for event in events
        .iter()
        .filter(|event| scope.admits(event) && matches!(event.payload(), Payload::SessionStarted))
    {
        let key = (event.source().name.clone(), event.source().path.clone());
        let at = event.created_at();
        latest
            .entry(key)
            .and_modify(|current| *current = (*current).max(at))
            .or_insert(at);
    }
    latest
}
