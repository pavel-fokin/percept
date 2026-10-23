use super::{Event, Name, Payload, ProjectId};
use crate::shared::Id;

pub type BoardId = Id<Board>;

/// A thinking surface in one project, created by a `BoardCreated` event.
/// Its name is one namespace with the project's maps; `Project::check_free` owns the rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub id: BoardId,
    pub project: ProjectId,
    pub name: Name,
}

impl Board {
    /// Every board in `events` in `BoardCreated` order.
    pub fn all(events: &[Event]) -> Vec<Self> {
        events
            .iter()
            .filter_map(|event| match &event.payload {
                Payload::BoardCreated { board, project, name } => Some(Self { id: *board, project: *project, name: name.clone() }),
                _ => None,
            })
            .collect()
    }

    /// The board of `project` in `events` named `name`, in any case.
    pub fn named(events: &[Event], project: ProjectId, name: &str) -> Option<Self> {
        let name = Name::new(name).ok()?;

        Self::all(events).into_iter().find(|b| b.project == project && b.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Actor, MapId, Places, Source};

    fn board(project: ProjectId, name: &str) -> Event {
        Event::board_created(Source::fixture(), Actor::Human, BoardId::new(), project, Name::new(name).unwrap())
    }

    #[test]
    fn boards_are_folded_from_their_events_in_log_order() {
        let project = ProjectId::new();
        let events = [
            board(project, "ideas"),
            Event::map_created(Source::fixture(), Actor::Human, MapId::new(), project, Name::new("decisions").unwrap(), "/maps/decisions.toml".into(), None, Places::fixture(project)),
            board(project, "plan"),
        ];

        let boards = Board::all(&events);

        let names: Vec<&str> = boards.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["ideas", "plan"]);
        assert!(boards.iter().all(|b| b.project == project));
    }

    #[test]
    fn a_board_is_found_in_its_project_by_name_in_any_case() {
        let (project, other) = (ProjectId::new(), ProjectId::new());
        let events = [board(project, "ideas")];

        let found = |project, name: &str| Board::named(&events, project, name).map(|b| b.name);

        assert_eq!(found(project, "ideas"), Some(Name::new("ideas").unwrap()));
        assert_eq!(found(project, "IDEAS"), Some(Name::new("ideas").unwrap()));
        assert_eq!(found(project, "plan"), None);
        assert_eq!(found(project, "not a name"), None);
        assert_eq!(found(other, "ideas"), None);
    }
}
