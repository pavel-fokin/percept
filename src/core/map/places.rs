use std::collections::BTreeSet;

use crate::core::{Board, BoardId, ProjectId};
use crate::shared::Result;

/// Where a map is shown: its project's map list, or a board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    Project(ProjectId),
    Board(BoardId),
}

/// Taking a place away would leave the map shown nowhere.
#[derive(Debug)]
pub struct LastPlace;

impl std::fmt::Display for LastPlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("this is the only place the map is shown")
    }
}

impl std::error::Error for LastPlace {}

/// The places a map is shown. A map is shown in at least one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places(BTreeSet<Place>);

impl Places {
    /// Refuses an empty set.
    pub fn new(places: impl IntoIterator<Item = Place>) -> Result<Self> {
        let places: BTreeSet<Place> = places.into_iter().collect();
        if places.is_empty() {
            return Err("a map is shown in at least one place".into());
        }

        Ok(Self(places))
    }

    /// These places and `place`.
    pub fn with(&self, place: Place) -> Self {
        let mut places = self.0.clone();
        places.insert(place);

        Self(places)
    }

    /// These places less `place`. Refuses to leave none, with `LastPlace`.
    pub fn without(&self, place: Place) -> Result<Self> {
        let mut places = self.0.clone();
        places.remove(&place);
        if places.is_empty() {
            return Err(LastPlace.into());
        }

        Ok(Self(places))
    }

    /// Refuses a project other than the map's own `project`, and a board that is not one of that project's `boards`.
    pub fn check_in(&self, project: ProjectId, boards: &[Board]) -> Result<()> {
        for place in self.iter() {
            match place {
                Place::Project(id) if id != project => return Err("a map is shown only in its own project".into()),
                Place::Board(id) if !boards.iter().any(|b| b.id == id && b.project == project) => {
                    return Err("a map is shown only on a board of its own project".into());
                }
                Place::Project(_) | Place::Board(_) => {}
            }
        }

        Ok(())
    }

    pub fn contains(&self, place: Place) -> bool {
        self.0.contains(&place)
    }

    pub fn iter(&self) -> impl Iterator<Item = Place> + '_ {
        self.0.iter().copied()
    }
}

#[cfg(test)]
impl Places {
    /// The places of a map shown in its `project` only.
    pub fn fixture(project: ProjectId) -> Self {
        Self::new([Place::Project(project)]).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_places_is_refused() {
        let result = Places::new([]);

        assert!(result.is_err());
    }

    #[test]
    fn with_adds_a_place_and_keeps_the_rest() {
        let (project, board) = (ProjectId::new(), BoardId::new());
        let places = Places::fixture(project);

        let added = places.with(Place::Board(board));

        assert!(added.contains(Place::Project(project)) && added.contains(Place::Board(board)));
        assert_eq!(added.with(Place::Board(board)), added);
        assert!(!places.contains(Place::Board(board)));
    }

    #[test]
    fn without_removes_a_place_but_never_the_last() {
        let (project, board) = (ProjectId::new(), BoardId::new());
        let places = Places::fixture(project).with(Place::Board(board));

        let removed = places.without(Place::Board(board)).unwrap();
        let unheld = removed.without(Place::Board(board)).unwrap();
        let last = removed.without(Place::Project(project));

        assert_eq!(removed, Places::fixture(project));
        assert_eq!(unheld, removed);
        assert!(last.unwrap_err().is::<LastPlace>());
    }

    #[test]
    fn check_in_accepts_the_own_project_and_its_boards_only() {
        let (project, other) = (ProjectId::new(), ProjectId::new());
        let own = Board { id: BoardId::new(), project, name: crate::core::Name::new("own").unwrap() };
        let foreign = Board { id: BoardId::new(), project: other, name: crate::core::Name::new("foreign").unwrap() };
        let boards = [own.clone(), foreign.clone()];
        let shown_at = |place| Places::fixture(project).with(place);

        assert!(shown_at(Place::Board(own.id)).check_in(project, &boards).is_ok());
        assert!(shown_at(Place::Board(foreign.id)).check_in(project, &boards).is_err());
        assert!(shown_at(Place::Board(BoardId::new())).check_in(project, &boards).is_err());
        assert!(shown_at(Place::Project(other)).check_in(project, &boards).is_err());
    }

    #[test]
    fn places_are_a_set_that_can_be_asked() {
        let (project, board) = (ProjectId::new(), BoardId::new());

        let places = Places::new([Place::Project(project), Place::Board(board), Place::Project(project)]).unwrap();

        assert_eq!(places.iter().count(), 2);
        assert!(places.contains(Place::Board(board)));
        assert!(!places.contains(Place::Board(BoardId::new())));
    }
}
