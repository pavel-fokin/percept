//! The one path a claim's standing changes through: `judge` folds the
//! map, resolves the node `maps confirm`, `maps dispute`, and the
//! review page's `/api/confirm` and `/api/dispute` all name, and
//! appends the event `event_of` builds for it - so the rule that a
//! user-written node carries no standing to judge lives once.
//! `judged_since_block`, with the capped-lines and header formatting
//! the rest of `percept hook`'s session-start block shares, is what
//! that block and the review page's foot both print - the same lines
//! from one function.

use crate::core::{
    Actor, Event, EventLog, Map, MapError, Node, NodeId, Schemas, Source, Standing,
};
use crate::shared::Timestamp;

use super::fold_map;

/// Resolves `s` against `map`, refusing a node the map does not hold or
/// one the human wrote themselves - a landmark, not a model's claim, so
/// it carries no standing to judge.
fn resolve_judged_node(map: &Map, s: &str) -> Result<NodeId, Box<dyn std::error::Error>> {
    let id = map.resolve_str(s)?;
    let node = map.node(id).expect("resolve_str returns a live node's id");
    if matches!(node.actor, Actor::Human(_)) {
        return Err(format!("{s} is the user's own; standing is for a model's claim").into());
    }
    Ok(id)
}

/// The one write path under `maps confirm`, `maps dispute`, and the
/// review page's `/api/confirm` and `/api/dispute`: folds the map,
/// resolves the judged node, appends the event `event_of` builds for
/// it, and returns it - `cli` prints the id, the server answers it.
pub(crate) fn judge(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    map_name: &str,
    node: &str,
    event_of: impl FnOnce(String, NodeId, Source) -> Event,
) -> Result<Event, Box<dyn std::error::Error>> {
    let map = fold_map(log, schemas, map_name, &source.scope())?;
    let node = resolve_judged_node(&map, node)?;
    let event = event_of(map_name.to_string(), node, source.clone());
    log.append(&event)?;
    Ok(event)
}

/// Whether `err` names a node id no map holds - the review page's
/// routes answer 404 for this and 400 for anything else a write path
/// rejects.
pub(crate) fn is_unknown_node(err: &(dyn std::error::Error + 'static)) -> bool {
    matches!(
        err.downcast_ref::<MapError>(),
        Some(MapError::NoSuchNode { .. } | MapError::UnknownShortId(_))
    )
}

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
