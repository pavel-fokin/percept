//! What the session-start block, the review page, and the Markdown
//! render share: the truncation rule, a node's line id, the wording of
//! its last change, and the session rule both a hook and the review
//! page cut their since by.

use std::path::Path;

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
/// path - recorded among `events`: `None` on a path's first session
/// with this client. The hook and the review
/// page both cut their since from it, then append a fresh one.
pub(crate) fn last_session(events: &[Event], source: &Source) -> Option<Timestamp> {
    events
        .iter()
        .filter(|event| {
            matches!(event.payload(), Payload::SessionStarted) && event.source() == source
        })
        .map(Event::created_at)
        .max()
}

/// The latest `session.started` recorded against `path`, from any
/// source name - unlike `last_session`, which cuts to one exact
/// `Source`. `start`'s render has no one client to cut to: a session
/// begun under Claude Code counts for a start run under Codex, since
/// both look at the same project.
pub(crate) fn last_session_at(events: &[Event], path: &Path) -> Option<Timestamp> {
    events
        .iter()
        .filter(|event| matches!(event.payload(), Payload::SessionStarted) && event.source().path == path)
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
