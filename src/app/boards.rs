use std::fmt;

use super::AppService;
use crate::core::{Actor, Board, BoardId, Event, EventStore, Name, Project, SchemaStore, Source};
use crate::shared::{Error, Result};

/// Why a board was not created.
#[derive(Debug)]
pub enum CreateBoardError {
    UnknownProject,
    InvalidName(Error),
    NameTaken(Name),
    Store(Error),
}

impl From<Error> for CreateBoardError {
    fn from(error: Error) -> Self {
        Self::Store(error)
    }
}

impl fmt::Display for CreateBoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProject => write!(f, "no such project"),
            Self::InvalidName(error) | Self::Store(error) => write!(f, "{error}"),
            Self::NameTaken(name) => write!(f, "the name {} is taken", name.as_str()),
        }
    }
}

impl<S: EventStore, M: SchemaStore> AppService<S, M> {
    /// Records a new board named `name` in the project named `project`, written by `client` from the project's root.
    /// A board's name is unique within its project.
    pub async fn create_board(&self, client: &str, actor: Actor, project: &str, name: &str) -> std::result::Result<Board, CreateBoardError> {
        let name = Name::new(name).map_err(CreateBoardError::InvalidName)?;

        // Held until return, so parallel creates cannot both pass the check.
        let _lock = self.store.lock().await?;

        let events = self.store.all().await?;
        let project = Project::all(&events).into_iter().find(|p| p.name.as_str() == project).ok_or(CreateBoardError::UnknownProject)?;
        if Board::all(&events).iter().any(|b| b.project == project.id && b.name == name) {
            return Err(CreateBoardError::NameTaken(name));
        }

        let board = BoardId::new();
        let source = Source { client: client.into(), path: project.root };
        self.store.append(&Event::board_created(source, actor, board, project.id, name.clone())).await?;

        Ok(Board { id: board, project: project.id, name })
    }

    /// Every board across all projects in `BoardCreated` order.
    pub async fn boards(&self) -> Result<Vec<Board>> {
        Ok(Board::all(&self.store.all().await?))
    }

    /// The board of the project named `project` that `board`, its name or its id, names.
    pub async fn board(&self, project: &str, board: &str) -> Result<Option<Board>> {
        let events = self.store.all().await?;

        let Some(project) = Project::all(&events).into_iter().find(|p| p.name.as_str() == project) else {
            return Ok(None);
        };

        Ok(Board::all(&events).into_iter().find(|b| b.project == project.id && b.is(board)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{add_project, in_project, Broken, Schemas};
    use crate::core::Payload;

    #[tokio::test]
    async fn a_created_board_is_folded_from_the_log() {
        let store = in_project("/work/a");
        let service = AppService::new(&store, Schemas);

        let created = service.create_board("percept-web", Actor::Human, "p0", "Ideas").await.unwrap();

        let boards = service.boards().await.unwrap();
        assert_eq!(boards, vec![created.clone()]);
        assert_eq!(created.name.as_str(), "ideas");
        let events = store.0.lock().unwrap();
        let event = events.last().unwrap();
        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.source, Source { client: "percept-web".into(), path: "/work/a".into() });
        assert!(matches!(&event.payload, Payload::BoardCreated { board, .. } if *board == created.id));
    }

    #[tokio::test]
    async fn a_taken_name_fails_whatever_its_case_but_another_project_may_reuse_it() {
        let store = in_project("/work/a");
        add_project(&store, "/work/b");
        let service = AppService::new(&store, Schemas);
        service.create_board("percept-web", Actor::Human, "p0", "ideas").await.unwrap();

        let taken = service.create_board("percept-web", Actor::Human, "p0", "IDEAS").await.unwrap_err();
        let other = service.create_board("percept-web", Actor::Human, "p1", "ideas").await;

        assert!(matches!(taken, CreateBoardError::NameTaken(_)));
        assert_eq!(taken.to_string(), "the name ideas is taken");
        assert!(other.is_ok());
        assert_eq!(service.boards().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn an_unknown_project_an_invalid_name_and_a_broken_store_write_nothing() {
        let store = in_project("/work/a");
        let service = AppService::new(&store, Schemas);

        let unknown = service.create_board("percept-web", Actor::Human, "nope", "ideas").await.unwrap_err();
        let invalid = service.create_board("percept-web", Actor::Human, "p0", "a b").await.unwrap_err();
        let reads_as_id = service.create_board("percept-web", Actor::Human, "p0", "0190a3b2-7c4e-7d2f-8a1b-1c2d3e4f5a6b").await.unwrap_err();
        let broken = AppService::new(Broken, Schemas).create_board("percept-web", Actor::Human, "p0", "ideas").await.unwrap_err();

        assert!(matches!(unknown, CreateBoardError::UnknownProject));
        assert!(matches!(invalid, CreateBoardError::InvalidName(_)));
        assert!(matches!(reads_as_id, CreateBoardError::InvalidName(_)));
        assert!(matches!(broken, CreateBoardError::Store(_)));
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_board_is_found_in_its_project_by_name_or_by_id() {
        let store = in_project("/work/a");
        add_project(&store, "/work/b");
        let service = AppService::new(&store, Schemas);
        let ideas = service.create_board("percept-web", Actor::Human, "p0", "ideas").await.unwrap();
        let id = serde_json::to_value(ideas.id).unwrap().as_str().unwrap().to_string();

        let by_name = service.board("p0", "Ideas").await.unwrap();
        let by_id = service.board("p0", &id).await.unwrap();
        let in_other_project = service.board("p1", "ideas").await.unwrap();
        let by_id_in_other_project = service.board("p1", &id).await.unwrap();
        let unknown_board = service.board("p0", "plan").await.unwrap();
        let unknown_project = service.board("nope", "ideas").await.unwrap();

        assert_eq!(by_name, Some(ideas.clone()));
        assert_eq!(by_id, Some(ideas));
        assert_eq!((in_other_project, by_id_in_other_project, unknown_board, unknown_project), (None, None, None, None));
    }

    #[tokio::test]
    async fn reads_pass_store_errors_through() {
        let service = AppService::new(Broken, Schemas);

        assert!(service.boards().await.is_err());
        assert!(service.board("p0", "ideas").await.is_err());
    }
}
