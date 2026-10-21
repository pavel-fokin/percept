use super::AppService;
use crate::core::{Actor, Board, BoardId, Event, EventStore, Name, Project, SchemaStore, Source};
use crate::shared::Result;

impl<S: EventStore, M: SchemaStore> AppService<S, M> {
    /// Records a new board in the project named `project`, written by `client` from the project's root.
    /// It is named `board-N`, N the smallest number no board of the project already uses.
    pub async fn create_board(&self, client: &str, actor: Actor, project: &str) -> Result<Option<Board>> {
        // Held until return, so parallel creates cannot both pick the same name.
        let _lock = self.store.lock().await?;

        let events = self.store.all().await?;
        let Some(project) = Project::named(&events, project) else {
            return Ok(None);
        };

        let taken: Vec<Name> = Board::all(&events).into_iter().filter(|b| b.project == project.id).map(|b| b.name).collect();
        let free = (1..).map(|n| format!("board-{n}")).find(|name| !taken.iter().any(|t| t.as_str() == name)).expect("an unbounded range has a free number");
        let name = Name::new(&free)?;

        let board = BoardId::new();
        let source = Source { client: client.into(), path: project.root };
        self.store.append(&Event::board_created(source, actor, board, project.id, name.clone())).await?;

        Ok(Some(Board { id: board, project: project.id, name }))
    }

    /// Every board across all projects in `BoardCreated` order.
    pub async fn boards(&self) -> Result<Vec<Board>> {
        Ok(Board::all(&self.store.all().await?))
    }

    /// The board named `board` in the project named `project`.
    pub async fn board(&self, project: &str, board: &str) -> Result<Option<Board>> {
        let events = self.store.all().await?;

        let Some(project) = Project::named(&events, project) else {
            return Ok(None);
        };

        Ok(Board::named(&events, project.id, board))
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

        let created = service.create_board("percept-web", Actor::Human, "p0").await.unwrap().unwrap();

        let boards = service.boards().await.unwrap();
        assert_eq!(boards, vec![created.clone()]);
        assert_eq!(created.name.as_str(), "board-1");
        let events = store.0.lock().unwrap();
        let event = events.last().unwrap();
        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.source, Source { client: "percept-web".into(), path: "/work/a".into() });
        assert!(matches!(&event.payload, Payload::BoardCreated { board, .. } if *board == created.id));
    }

    #[tokio::test]
    async fn two_creates_are_numbered_in_turn_and_per_project() {
        let store = in_project("/work/a");
        add_project(&store, "/work/b");
        let service = AppService::new(&store, Schemas);

        let first = service.create_board("percept-web", Actor::Human, "p0").await.unwrap().unwrap();
        let second = service.create_board("percept-web", Actor::Human, "p0").await.unwrap().unwrap();
        let other = service.create_board("percept-web", Actor::Human, "p1").await.unwrap().unwrap();

        assert_eq!(first.name.as_str(), "board-1");
        assert_eq!(second.name.as_str(), "board-2");
        assert_eq!(other.name.as_str(), "board-1");
    }

    #[tokio::test]
    async fn a_free_gap_is_reused() {
        let store = in_project("/work/a");
        let project = Project::named(&store.0.lock().unwrap(), "p0").unwrap();
        for name in ["board-1", "board-3"] {
            let source = Source { client: "percept-web".into(), path: "/work/a".into() };
            store.0.lock().unwrap().push(Event::board_created(source, Actor::Human, BoardId::new(), project.id, Name::new(name).unwrap()));
        }
        let service = AppService::new(&store, Schemas);

        let created = service.create_board("percept-web", Actor::Human, "p0").await.unwrap().unwrap();

        assert_eq!(created.name.as_str(), "board-2");
    }

    #[tokio::test]
    async fn an_unknown_project_and_a_broken_store_write_nothing() {
        let store = in_project("/work/a");
        let service = AppService::new(&store, Schemas);

        let unknown = service.create_board("percept-web", Actor::Human, "nope").await.unwrap();
        let broken = AppService::new(Broken, Schemas).create_board("percept-web", Actor::Human, "p0").await;

        assert_eq!(unknown, None);
        assert!(broken.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_board_is_found_in_its_project_by_name_only() {
        let store = in_project("/work/a");
        add_project(&store, "/work/b");
        let service = AppService::new(&store, Schemas);
        let first = service.create_board("percept-web", Actor::Human, "p0").await.unwrap().unwrap();

        let by_name = service.board("p0", "Board-1").await.unwrap();
        let in_other_project = service.board("p1", "board-1").await.unwrap();
        let unknown_board = service.board("p0", "board-2").await.unwrap();
        let unknown_project = service.board("nope", "board-1").await.unwrap();

        assert_eq!(by_name, Some(first));
        assert_eq!((in_other_project, unknown_board, unknown_project), (None, None, None));
    }

    #[tokio::test]
    async fn reads_pass_store_errors_through() {
        let service = AppService::new(Broken, Schemas);

        assert!(service.boards().await.is_err());
        assert!(service.board("p0", "ideas").await.is_err());
    }
}
