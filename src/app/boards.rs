use super::AppService;
use crate::core::{Actor, Board, BoardId, Event, EventStore, Map, Name, Place, Project, ProjectId, SkillStore, Source};
use crate::shared::Result;

/// A board of a project with the maps shown on it, each with its nodes and edges.
pub struct BoardContents {
    pub project: Project,
    pub board: Board,
    pub maps: Vec<Map>,
}

impl<S: EventStore, M: SkillStore> AppService<S, M> {
    /// Records a new board in the project named `project`, written from `source`.
    /// It is named `board`, or `board-N` when taken, N the smallest number no board or map of the project already uses.
    pub async fn create_board(&self, source: Source, actor: Actor, project: &str) -> Result<Option<Board>> {
        let events = self.store.all().await?;
        let Some(project) = Project::named(&events, project) else {
            return Ok(None);
        };

        let name = Name::free(&Name::new("board")?, &Project::taken(&events, project.id));

        self.append_board(source, actor, project.id, name).await.map(Some)
    }

    /// Records a new board named `name` in the project. No board or map of the project may have the name.
    pub async fn create_named_board(&self, source: Source, actor: Actor, project: ProjectId, name: &str) -> Result<Board> {
        let name = Name::new(name)?;

        let events = self.store.all().await?;
        Project::check_free(&events, project, &name)?;

        self.append_board(source, actor, project, name).await
    }

    async fn append_board(&self, source: Source, actor: Actor, project: ProjectId, name: Name) -> Result<Board> {
        let board = BoardId::new();
        self.store.append(&Event::board_created(source, actor, board, project, name.clone())).await?;

        Ok(Board { id: board, project, name })
    }

    /// Every board across all projects in `BoardCreated` order.
    pub async fn boards(&self) -> Result<Vec<Board>> {
        Ok(Board::all(&self.store.all().await?))
    }

    /// The boards of the project in `BoardCreated` order, each with the maps shown on it, from one read of the log.
    pub async fn project_boards(&self, project: ProjectId) -> Result<Vec<(Board, Vec<Map>)>> {
        let events = self.store.all().await?;

        let boards = Board::all(&events).into_iter().filter(|b| b.project == project);

        Ok(boards
            .map(|board| {
                let maps = Map::at(&events, Place::Board(board.id));

                (board, maps)
            })
            .collect())
    }

    /// The board named `board` in the project named `project`, with the maps shown on it, from one read of the log.
    pub async fn board_contents(&self, project: &str, board: &str) -> Result<Option<BoardContents>> {
        let events = self.store.all().await?;

        let Some(project) = Project::named(&events, project) else {
            return Ok(None);
        };
        let Some(board) = Board::named(&events, project.id, board) else {
            return Ok(None);
        };

        let maps = Map::at(&events, Place::Board(board.id));

        Ok(Some(BoardContents { project, board, maps }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{add_project, id_at, in_project, Broken, Skills};
    use std::path::Path;
    use crate::core::Payload;

    #[tokio::test]
    async fn a_created_board_is_folded_from_the_log() {
        let store = in_project("/work/a");
        let service = AppService::new(&store, Skills);

        let created = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();

        let boards = service.boards().await.unwrap();
        assert_eq!(boards, vec![created.clone()]);
        assert_eq!(created.name.as_str(), "board");
        let events = store.0.lock().unwrap();
        let event = events.last().unwrap();
        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.source, Source::Web);
        assert!(matches!(&event.payload, Payload::BoardCreated { board, .. } if *board == created.id));
    }

    #[tokio::test]
    async fn two_creates_are_numbered_in_turn_and_per_project() {
        let store = in_project("/work/a");
        add_project(&store, "/work/b");
        let service = AppService::new(&store, Skills);

        let first = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();
        let second = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();
        let other = service.create_board(Source::Web, Actor::Human, "p1").await.unwrap().unwrap();

        assert_eq!(first.name.as_str(), "board");
        assert_eq!(second.name.as_str(), "board-1");
        assert_eq!(other.name.as_str(), "board");
    }

    #[tokio::test]
    async fn a_named_board_is_recorded_and_a_taken_name_is_refused() {
        let store = in_project("/work/a");
        let project = id_at(&store, "/work/a");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::Web, Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();

        let board = service.create_named_board(Source::Web, Actor::Agent, project, "Storage").await.unwrap();
        let duplicate = service.create_named_board(Source::Web, Actor::Agent, project, "storage").await;
        let like_a_map = service.create_named_board(Source::Web, Actor::Agent, project, "decisions").await;

        assert_eq!(board.name.as_str(), "storage");
        assert_eq!(service.boards().await.unwrap(), vec![board]);
        assert_eq!(duplicate.unwrap_err().to_string(), "this project already has a storage board");
        assert_eq!(like_a_map.unwrap_err().to_string(), "this project already has a decisions map");
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn an_auto_named_board_skips_the_names_of_maps() {
        let store = in_project("/work/a");
        let project = id_at(&store, "/work/a");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::Web, Actor::Human, project, "board", Some(Path::new("/skills/decisions"))).await.unwrap();

        let created = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();

        assert_eq!(created.name.as_str(), "board-1");
    }

    #[tokio::test]
    async fn a_free_gap_is_reused() {
        let store = in_project("/work/a");
        let project = Project::named(&store.0.lock().unwrap(), "p0").unwrap();
        for name in ["board", "board-2"] {
            let source = Source::Web;
            store.0.lock().unwrap().push(Event::board_created(source, Actor::Human, BoardId::new(), project.id, Name::new(name).unwrap()));
        }
        let service = AppService::new(&store, Skills);

        let created = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();

        assert_eq!(created.name.as_str(), "board-1");
    }

    #[tokio::test]
    async fn an_unknown_project_and_a_broken_store_write_nothing() {
        let store = in_project("/work/a");
        let service = AppService::new(&store, Skills);

        let unknown = service.create_board(Source::Web, Actor::Human, "nope").await.unwrap();
        let broken = AppService::new(Broken, Skills).create_board(Source::Web, Actor::Human, "p0").await;

        assert_eq!(unknown, None);
        assert!(broken.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_board_is_found_in_its_project_by_name_only() {
        let store = in_project("/work/a");
        add_project(&store, "/work/b");
        let service = AppService::new(&store, Skills);
        let first = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();

        let found = async |project, board| service.board_contents(project, board).await.unwrap().map(|c| c.board);

        let by_name = found("P0", "Board").await;
        let in_other_project = found("p1", "board").await;
        let unknown_board = found("p0", "board-1").await;
        let unknown_project = found("nope", "board").await;

        assert_eq!(by_name, Some(first));
        assert_eq!((in_other_project, unknown_board, unknown_project), (None, None, None));
    }

    #[tokio::test]
    async fn a_board_holds_the_maps_placed_on_it() {
        let store = in_project("/work/a");
        let project = id_at(&store, "/work/a");
        let service = AppService::new(&store, Skills);
        let board = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();
        for name in ["decisions", "tasks"] {
            service.create_map(Source::Web, Actor::Human, project, name, Some(Path::new(&format!("/skills/{name}")))).await.unwrap();
        }
        let decisions = service.map_id("p0", "decisions").await.unwrap().unwrap();
        service.place_map(Source::Web, Actor::Human, decisions, Place::Board(board.id)).await.unwrap();

        let contents = service.board_contents("p0", "board").await.unwrap().unwrap();
        let unknown = service.board_contents("p0", "nope").await.unwrap();

        assert_eq!(contents.board, board);
        assert_eq!(contents.maps.iter().map(|m| m.id).collect::<Vec<_>>(), vec![decisions]);
        assert!(unknown.is_none());
    }

    #[tokio::test]
    async fn a_project_lists_its_boards_with_their_maps() {
        let store = in_project("/work/a");
        add_project(&store, "/work/b");
        let project = id_at(&store, "/work/a");
        let service = AppService::new(&store, Skills);
        let first = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();
        let second = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap();
        service.create_board(Source::Web, Actor::Human, "p1").await.unwrap();
        service.create_map(Source::Web, Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        let decisions = service.map_id("p0", "decisions").await.unwrap().unwrap();
        service.place_map(Source::Web, Actor::Human, decisions, Place::Board(second.id)).await.unwrap();

        let boards = service.project_boards(project).await.unwrap();

        let found: Vec<(BoardId, usize)> = boards.iter().map(|(board, maps)| (board.id, maps.len())).collect();
        assert_eq!(found, vec![(first.id, 0), (second.id, 1)]);
        assert!(service.project_boards(ProjectId::new()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reads_pass_store_errors_through() {
        let service = AppService::new(Broken, Skills);

        assert!(service.boards().await.is_err());
        assert!(service.board_contents("p0", "ideas").await.is_err());
        assert!(service.project_boards(ProjectId::new()).await.is_err());
    }
}
