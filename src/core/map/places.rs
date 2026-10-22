use std::collections::BTreeSet;

use crate::core::{BoardId, ProjectId};
use crate::shared::Result;

/// Where a map is shown: its project's map list, or a board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    Project(ProjectId),
    Board(BoardId),
}

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
    fn places_are_a_set_that_can_be_asked() {
        let (project, board) = (ProjectId::new(), BoardId::new());

        let places = Places::new([Place::Project(project), Place::Board(board), Place::Project(project)]).unwrap();

        assert_eq!(places.iter().count(), 2);
        assert!(places.contains(Place::Board(board)));
        assert!(!places.contains(Place::Board(BoardId::new())));
    }
}
