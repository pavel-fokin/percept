//! What the session-start block, the review page, and the Markdown
//! render share: the truncation rule, a node's line id, the wording of
//! its last change, and the session rule both a hook and the review
//! page cut their since by.

use crate::core::{Event, Map, Node, Payload, Source, Written};
use crate::shared::Timestamp;

/// How many lines of a gained or changed list a block shows
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

/// The latest `session.started` this exact source - client name and
/// project path - recorded, among `events` in its scope: `None` on a
/// project's first session with this client. The hook and the review
/// page both cut their since from it, then append a fresh one.
pub(crate) fn last_session(events: &[Event], source: &Source) -> Option<Timestamp> {
    let scope = source.scope();
    events
        .iter()
        .filter(|event| {
            scope.admits(event)
                && matches!(event.payload(), Payload::SessionStarted)
                && event.source().name == source.name
                && event.source().path == source.path
        })
        .map(Event::created_at)
        .max()
}

/// `changed by <actor>`, with `: "<why>"` when the change carried one -
/// `None` when `node`'s last change is its addition. The one wording
/// every render of a node's last change uses.
pub(crate) fn changed_line(node: &Node) -> Option<String> {
    if node.history.len() <= 1 {
        return None;
    }
    let mut line = format!("changed by {}", node.changed().actor.name());
    if let Some(why) = &node.changed().why {
        line.push_str(&format!(": {why:?}"));
    }
    Some(line)
}
