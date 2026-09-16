//! `GET /api/projects` - one entry per project the log carries, each
//! with the maps it keeps, their size, and what each gained since that
//! project's last session. Unlike `server::events`, not scoped to this
//! server's own project: the log under `PERCEPT_HOME` holds every
//! project's events, and this route's whole purpose is to cross them.

use std::path::Path;

use serde_json::{json, Value};

use crate::core::{Event, EventLog};
use crate::mapstore::{self, gained, last_session, of_path, project_name};
use crate::server::events::Error;
use crate::shared::Timestamp;

/// `GET /api/projects`'s body: `projects`, ordered newest
/// `last_active` first. A project whose root declares no schema still
/// appears, with `maps: []` - another checkout may be gone, or never
/// initialised. `opened` is when this view started: a map's `gained`
/// is measured from the last session that began before it, since
/// opening the view recorded a session of its own.
pub fn list(log: &dyn EventLog, opened: Timestamp) -> Result<Value, Error> {
    let events = log.load().map_err(|err| Error::Internal(err.to_string()))?;

    let mut projects: Vec<(Timestamp, Value)> = mapstore::paths(&events)
        .into_iter()
        .map(|path| project(&path, &events, opened))
        .collect::<Result<_, Error>>()?;
    projects.sort_by(|(a, _), (b, _)| b.cmp(a));

    let projects: Vec<Value> = projects.into_iter().map(|(_, project)| project).collect();
    Ok(json!({ "projects": projects }))
}

/// One `projects` entry for `path`, cut from `events`: its own events,
/// the newest of their `created_at`, and every map its own schemas
/// declare - `[]` when it declares none. Returns the project's
/// `last_active` alongside, for `list` to sort by. Counts and maps are
/// live; only the session `gained` counts from is held back to
/// `opened`.
fn project(path: &Path, events: &[Event], opened: Timestamp) -> Result<(Timestamp, Value), Error> {
    let own: Vec<&Event> = of_path(events, path).collect();
    // `paths` only names a path some event carries, so `own` is never
    // empty and `last_active` always has a newest to report.
    let last_active = own.iter().map(|event| event.created_at()).max().expect("own is never empty");
    let since = last_session(own.iter().copied().filter(|event| event.created_at() < opened));

    let schemas = mapstore::load_schemas(path).map_err(|err| Error::Internal(err.to_string()))?;
    let maps = schemas
        .fold_all(own.iter().copied())
        .map_err(|err| Error::Internal(err.to_string()))?;
    let maps: Vec<Value> = maps
        .iter()
        .map(|map| {
            let gained = since.map_or(0, |since| gained(map, since).len());
            json!({
                "name": map.schema().name,
                "headlines": map.headlines().count(),
                "gained": gained,
            })
        })
        .collect();

    let entry = json!({
        "name": project_name(path),
        "path": path.to_string_lossy(),
        "events": own.len(),
        "last_active": last_active.to_string(),
        "maps": maps,
    });
    Ok((last_active, entry))
}

#[cfg(test)]
mod tests;
