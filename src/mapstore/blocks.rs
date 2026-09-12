//! What the session-start block, the review page, and the Markdown
//! render share: the truncation rule, a node's line id, the wording of
//! its last change, and the session rule both a hook and the review
//! page cut their since by.

use crate::core::{Event, Map, Node, Payload, Written};
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

/// The short id `node` has on `map`, or a `kind:name` fallback for the
/// unexpected case a headline node carries none.
pub(crate) fn line_id(map: &Map, node: &Node) -> String {
    map.short_id(node.id)
        .unwrap_or_else(|| format!("{}:{}", node.kind, node.name))
}

/// The latest `session.started` among `events`, already cut to whose
/// sessions count: the hook and the review page pass their own exact
/// source, and record a fresh one after; `percept start` from the
/// shell records none, so it passes `of_path` - the last look by
/// anyone here. `None` when no session has started.
pub(crate) fn last_session<'a>(events: impl IntoIterator<Item = &'a Event>) -> Option<Timestamp> {
    events
        .into_iter()
        .filter(|event| matches!(event.payload(), Payload::SessionStarted))
        .map(Event::created_at)
        .max()
}

/// `changed by <actor>`, with `: "<why>"` when the change carried one -
/// `None` when `node`'s last change is its addition. The one wording
/// every render of a node's last change uses.
pub(crate) fn changed_line(node: &Node) -> Option<String> {
    let [_, .., changed] = node.history() else {
        return None;
    };
    let mut line = format!("changed by {}", changed.actor.name());
    if let Some(why) = &changed.why {
        line.push_str(&format!(": {why:?}"));
    }
    Some(line)
}
