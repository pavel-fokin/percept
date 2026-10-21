use std::path::{Path, PathBuf};

use super::{Event, Name, Payload};
use crate::shared::Id;

pub type ProjectId = Id<Project>;

/// A folder percept tracks, created by a `ProjectCreated` event.
/// Its root is the `source.path` of that event.
pub struct Project {
    pub id: ProjectId,
    pub name: Name,
    pub root: PathBuf,
}

impl Project {
    /// Every project in `events` in `ProjectCreated` order.
    pub fn all(events: &[Event]) -> Vec<Self> {
        events
            .iter()
            .filter_map(|event| match &event.payload {
                Payload::ProjectCreated { project, name } => {
                    Some(Self { id: *project, name: name.clone(), root: event.source.path.clone() })
                }
                _ => None,
            })
            .collect()
    }

    /// The project in `events` whose root is the nearest at or above `path`, as git finds its root.
    pub fn containing(events: &[Event], path: &Path) -> Option<Self> {
        Self::all(events).into_iter().filter(|p| path.starts_with(&p.root)).max_by_key(|p| p.root.components().count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Actor, Source};

    fn source(path: &str) -> Source {
        Source { client: "percept-cli".into(), path: path.into() }
    }

    #[test]
    fn the_nearest_root_at_or_above_a_path_wins() {
        let events = [
            Event::project_created(source("/work/app"), Actor::Human, ProjectId::new(), Name::new("app").unwrap()),
            Event::project_created(source("/work/app/inner"), Actor::Human, ProjectId::new(), Name::new("inner").unwrap()),
            Event::project_created(source("/work/apple"), Actor::Human, ProjectId::new(), Name::new("apple").unwrap()),
        ];

        let name = |path: &str| Project::containing(&events, Path::new(path)).map(|p| p.name.as_str().to_string());

        assert_eq!(name("/work/app"), Some("app".to_string()));
        assert_eq!(name("/work/app/src/deep"), Some("app".to_string()));
        assert_eq!(name("/work/app/inner/src"), Some("inner".to_string()));
        assert_eq!(name("/work/apple/x"), Some("apple".to_string()));
        assert_eq!(name("/work"), None);
        assert_eq!(name("/elsewhere"), None);
    }

    #[test]
    fn projects_are_folded_from_their_events_in_log_order() {
        let events = [
            Event::project_created(source("/a"), Actor::Human, ProjectId::new(), Name::new("a").unwrap()),
            Event::map_created(Source::fixture(), Actor::Human, crate::core::MapId::new(), ProjectId::new(), "decisions"),
            Event::project_created(source("/b"), Actor::Human, ProjectId::new(), Name::new("b").unwrap()),
        ];

        let projects = Project::all(&events);

        let found: Vec<(&str, &Path)> = projects.iter().map(|p| (p.name.as_str(), p.root.as_path())).collect();
        assert_eq!(found, vec![("a", Path::new("/a")), ("b", Path::new("/b"))]);
    }
}
