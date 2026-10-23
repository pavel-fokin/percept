use std::path::{Path, PathBuf};

use super::{Board, Event, Map, Name, Payload};
use crate::shared::{Id, Result};

pub type ProjectId = Id<Project>;

/// A project percept tracks, created by a `ProjectCreated` event.
/// Its root, when it has one, is the folder the payload names, not where the event was written.
#[derive(Debug)]
pub struct Project {
    pub id: ProjectId,
    pub name: Name,
    pub root: Option<PathBuf>,
}

impl Project {
    /// Every project in `events` in `ProjectCreated` order.
    pub fn all(events: &[Event]) -> Vec<Self> {
        events
            .iter()
            .filter_map(|event| match &event.payload {
                Payload::ProjectCreated { project, name, root } => Some(Self { id: *project, name: name.clone(), root: root.clone() }),
                _ => None,
            })
            .collect()
    }

    /// The project in `events` named `name`, in any case.
    pub fn named(events: &[Event], name: &str) -> Option<Self> {
        let name = Name::new(name).ok()?;

        Self::all(events).into_iter().find(|p| p.name == name)
    }

    /// The project in `events` whose root is the nearest at or above `path`, as git finds its root.
    pub fn containing(events: &[Event], path: &Path) -> Option<Self> {
        Self::all(events)
            .into_iter()
            .filter(|p| p.root.as_deref().is_some_and(|root| path.starts_with(root)))
            .max_by_key(|p| p.root.as_ref().map(|root| root.components().count()))
    }

    /// The names the boards and maps of `project` hold. They share one namespace, so a path names at most one of them.
    pub fn taken(events: &[Event], project: ProjectId) -> Vec<Name> {
        let boards = Board::all(events).into_iter().filter(|b| b.project == project).map(|b| b.name);
        let maps = Map::all(events).into_iter().filter(|m| m.project == project).map(|m| m.name);

        boards.chain(maps).collect()
    }

    /// Refuses `name` when a board or a map of `project` already has it.
    pub fn check_free(events: &[Event], project: ProjectId, name: &Name) -> Result<()> {
        if Map::named(events, project, name.as_str()).is_some() {
            return Err(format!("this project already has a {} map", name.as_str()).into());
        }
        if Board::named(events, project, name.as_str()).is_some() {
            return Err(format!("this project already has a {} board", name.as_str()).into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Actor, Source};

    fn source(path: &str) -> Source {
        Source::Checkout { client: "percept-cli".into(), path: path.into() }
    }

    #[test]
    fn the_nearest_root_at_or_above_a_path_wins() {
        let events = [
            Event::project_created(source("/work/app"), Actor::Human, ProjectId::new(), Name::new("app").unwrap(), Some("/work/app".into())),
            Event::project_created(source("/work/app/inner"), Actor::Human, ProjectId::new(), Name::new("inner").unwrap(), Some("/work/app/inner".into())),
            Event::project_created(source("/work/apple"), Actor::Human, ProjectId::new(), Name::new("apple").unwrap(), Some("/work/apple".into())),
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
    fn a_project_is_found_by_its_name_in_any_case() {
        let events = [Event::project_created(source("/work/app"), Actor::Human, ProjectId::new(), Name::new("app").unwrap(), Some("/work/app".into()))];

        let found = |name: &str| Project::named(&events, name).map(|p| p.root);

        assert_eq!(found("app"), Some(Some("/work/app".into())));
        assert_eq!(found("APP"), Some(Some("/work/app".into())));
        assert_eq!(found("other"), None);
        assert_eq!(found("not a name"), None);
    }

    #[test]
    fn projects_are_folded_from_their_events_in_log_order() {
        let events = [
            Event::project_created(source("/a"), Actor::Human, ProjectId::new(), Name::new("a").unwrap(), Some("/a".into())),
            Event::map_created(Source::fixture(), Actor::Human, crate::core::MapId::new(), ProjectId::new(), Name::new("decisions").unwrap(), "/maps/decisions.toml".into(), None, crate::core::Places::fixture(ProjectId::new())),
            Event::project_created(source("/b"), Actor::Human, ProjectId::new(), Name::new("b").unwrap(), Some("/b".into())),
        ];

        let projects = Project::all(&events);

        let found: Vec<(&str, &Path)> = projects.iter().map(|p| (p.name.as_str(), p.root.as_deref().unwrap())).collect();
        assert_eq!(found, vec![("a", Path::new("/a")), ("b", Path::new("/b"))]);
    }

    #[test]
    fn a_project_root_comes_from_the_payload_not_the_event_source() {
        let events = [Event::project_created(source("/elsewhere"), Actor::Human, ProjectId::new(), Name::new("app").unwrap(), Some("/work/app".into()))];

        let projects = Project::all(&events);

        assert_eq!(projects[0].root.as_deref(), Some(Path::new("/work/app")));
    }

    #[test]
    fn a_project_without_a_root_contains_no_path() {
        let events = [Event::project_created(Source::Web, Actor::Human, ProjectId::new(), Name::new("web").unwrap(), None)];

        assert!(Project::containing(&events, Path::new("/")).is_none());
        assert!(Project::containing(&events, Path::new("/work/app")).is_none());
    }

    #[test]
    fn boards_and_maps_share_one_namespace_per_project() {
        let (project, other) = (ProjectId::new(), ProjectId::new());
        let name = |s: &str| Name::new(s).unwrap();
        let events = [
            Event::board_created(Source::fixture(), Actor::Human, crate::core::BoardId::new(), project, name("ideas")),
            Event::map_created(Source::fixture(), Actor::Human, crate::core::MapId::new(), project, name("decisions"), "/s".into(), None, crate::core::Places::fixture(project)),
        ];

        let check = |project, text: &str| Project::check_free(&events, project, &name(text)).map_err(|e| e.to_string());

        assert_eq!(check(project, "decisions"), Err("this project already has a decisions map".into()));
        assert_eq!(check(project, "ideas"), Err("this project already has a ideas board".into()));
        assert_eq!(check(project, "plan"), Ok(()));
        assert_eq!(check(other, "ideas"), Ok(()));
        assert_eq!(Project::taken(&events, project), vec![name("ideas"), name("decisions")]);
        assert!(Project::taken(&events, other).is_empty());
    }
}
