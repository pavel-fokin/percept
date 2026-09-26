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
/// initialised - and so does one whose schemas percept cannot read,
/// with the reason in `maps_error`, `null` for every project it
/// could. `opened` is when this view started: a map's `gained`
/// is measured from the last session that began before it, since
/// opening the view recorded a session of its own.
pub fn list(log: &dyn EventLog, opened: Timestamp) -> Result<Value, Error> {
    let events = log.load().map_err(|err| Error::Internal(err.to_string()))?;

    let mut projects: Vec<(Timestamp, Value)> = mapstore::paths(&events)
        .into_iter()
        .map(|path| project(&path, &events, opened))
        .collect();
    projects.sort_by(|(a, _), (b, _)| b.cmp(a));

    let projects: Vec<Value> = projects.into_iter().map(|(_, project)| project).collect();
    Ok(json!({ "projects": projects }))
}

/// One `projects` entry for `path`, cut from `events`: its own events,
/// the newest of their `created_at`, and every map its own schemas
/// declare - `[]` when it declares none, or when `maps_error` says why
/// they could not be read. Returns the project's `last_active`
/// alongside, for `list` to sort by. Counts and maps are live; only
/// the session `gained` counts from is held back to `opened`.
fn project(path: &Path, events: &[Event], opened: Timestamp) -> (Timestamp, Value) {
    let own: Vec<&Event> = of_path(events, path).collect();
    // `paths` only names a path some event carries, so `own` is never
    // empty and `last_active` always has a newest to report.
    let last_active = own.iter().map(|event| event.created_at()).max().expect("own is never empty");
    let since = last_session(own.iter().copied().filter(|event| event.created_at() < opened));

    let (maps, maps_error) = match maps(path, events, since) {
        Ok(maps) => (maps, Value::Null),
        Err(reason) => (Vec::new(), Value::String(reason)),
    };

    let entry = json!({
        "name": project_name(path),
        "path": path.to_string_lossy(),
        "events": own.len(),
        "last_active": last_active.to_string(),
        "maps": maps,
        "maps_error": maps_error,
    });
    (last_active, entry)
}

/// Every map `path`'s own schemas declare, folded by
/// `mapstore::fold_all_at`, with what each gained since `since`. A
/// schema percept cannot load, or a map that will not fold, answers
/// the reason rather than failing the request: the index crosses
/// projects, so one project's broken file would otherwise leave every
/// other project unreadable too.
fn maps(path: &Path, events: &[Event], since: Option<Timestamp>) -> Result<Vec<Value>, String> {
    // The web view stays project-only: no home, so no global schema.
    let schemas = mapstore::load_schemas(Some(path), None).map_err(|err| err.to_string())?;
    let maps = mapstore::fold_all_at(&schemas, events, path).map_err(|err| err.to_string())?;
    Ok(maps
        .iter()
        .map(|map| {
            let gained = since.map_or(0, |since| gained(map, since).len());
            json!({
                "id": map.id().as_uuid().to_string(),
                "name": map.schema().name(),
                "nodes": map.nodes().len(),
                "gained": gained,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests;
