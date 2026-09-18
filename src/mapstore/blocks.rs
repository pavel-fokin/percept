//! What the session-start block, the web view, and the Markdown
//! render share: the truncation rule, a node's line id, the wording of
//! its last change, the session rule both a hook and the review page
//! cut their since by, a project's display name, and which headline
//! nodes counted as gained since a cut.

use std::path::Path;

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
/// sessions count: the hook and the web view pass their own exact
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

/// `changed by <actor>` - `None` when `node`'s last change is its
/// addition. The one wording every render of a node's last change
/// uses.
pub(crate) fn changed_line(node: &Node) -> Option<String> {
    let [_, .., changed] = node.history() else {
        return None;
    };
    Some(format!("changed by {}", changed.actor.name()))
}

/// `root`'s last path component, the name a reader knows the project
/// by - falling back to the whole path on the rare root with none.
pub(crate) fn project_name(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

/// The headline nodes of `map` whose last change happened at or after
/// `since` - a node's last change is compared directly, not
/// `Map::since`, which would also surface an older node a fresh edge
/// only touched.
pub(crate) fn gained(map: &Map, since: Timestamp) -> Vec<&Node> {
    map.headlines().filter(|node| node.changed().at >= since).collect()
}
