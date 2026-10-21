use super::{Event, Name, Payload, ProjectId};
use crate::shared::Id;

pub type BoardId = Id<Board>;

/// A thinking surface in one project, created by a `BoardCreated` event.
/// Its name is unique within the project.
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

    /// Whether `key`, a board's id or its name, names this board.
    /// A name never reads as an id, so a key matches by one or the other.
    pub fn is(&self, key: &str) -> bool {
        match key.parse::<BoardId>() {
            Ok(id) => self.id == id,
            Err(_) => Name::new(key).is_ok_and(|name| self.name == name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Actor, MapId, Source};

    fn board(project: ProjectId, name: &str) -> Event {
        Event::board_created(Source::fixture(), Actor::Human, BoardId::new(), project, Name::new(name).unwrap())
    }

    fn text(id: BoardId) -> String {
        serde_json::to_value(id).unwrap().as_str().unwrap().to_string()
    }

    #[test]
    fn boards_are_folded_from_their_events_in_log_order() {
        let project = ProjectId::new();
        let events = [
            board(project, "ideas"),
            Event::map_created(Source::fixture(), Actor::Human, MapId::new(), project, "decisions"),
            board(project, "plan"),
        ];

        let boards = Board::all(&events);

        let names: Vec<&str> = boards.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["ideas", "plan"]);
        assert!(boards.iter().all(|b| b.project == project));
    }

    #[test]
    fn a_board_is_named_by_its_id_or_its_name_in_any_case() {
        let boards = Board::all(&[board(ProjectId::new(), "ideas")]);
        let ideas = &boards[0];

        assert!(ideas.is("ideas"));
        assert!(ideas.is("IDEAS"));
        assert!(ideas.is(&text(ideas.id)));
        assert!(!ideas.is("plan"));
        assert!(!ideas.is(&text(BoardId::new())));
        assert!(!ideas.is("not a name"));
    }
}
