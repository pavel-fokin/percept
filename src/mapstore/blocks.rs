//! What the web view and the Markdown render share: the wording of a
//! node's last change, the session rule the review page cuts its
//! since by, a project's display name, and which nodes counted as
//! gained since a cut.

use std::path::Path;

use crate::core::{Event, Map, Node, Payload, Written};
use crate::shared::Timestamp;

/// The latest `session.started` among `events`, already cut to whose
/// sessions count: the web view passes its own exact source. `None`
/// when no session has started.
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

/// The nodes of `map` whose last change happened at or after
/// `since` - a node's last change is compared directly, not
/// `Map::since`, which would also surface an older node a fresh edge
/// only touched.
pub(crate) fn gained(map: &Map, since: Timestamp) -> Vec<&Node> {
    map.nodes().iter().filter(|node| node.changed().at >= since).collect()
}
