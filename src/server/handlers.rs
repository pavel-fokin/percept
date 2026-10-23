use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use serde::Deserialize;

use super::responses::{self, BoardDetail, BoardSummary, Data, MapDetail, MapSummary, ProjectSummary, Status};
use super::Api;
use crate::app::Taken;
use crate::core::{Actor, EventLog, LastPlace, Name, Place, SessionId, SkillStore, Source};
use crate::shared::Error;

/// The body of `POST /api/projects`.
#[derive(Deserialize)]
pub(super) struct NewProject {
    name: String,
}

/// The body of `POST /api/projects/{project}/boards/{board}/maps`: the name of a map of the project.
#[derive(Deserialize)]
pub(super) struct PlacedMap {
    map: String,
}

type Reply<T> = Result<Json<Data<T>>, ApiError>;

pub(super) enum ApiError {
    NotFound,
    /// The request is wrong; the message says how.
    BadRequest(String),
    /// The request clashes with what exists; the message says with what.
    Conflict(String),
    Internal(Error),
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        let error = match error.downcast::<Taken>() {
            Ok(taken) => return Self::Conflict(taken.to_string()),
            Err(error) => error,
        };

        match error.downcast::<LastPlace>() {
            Ok(last) => Self::Conflict(last.to_string()),
            Err(error) => Self::Internal(error),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message).into_response(),
            Self::Conflict(message) => (StatusCode::CONFLICT, message).into_response(),
            Self::Internal(error) => {
                eprintln!("percept: {error}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

impl<L, M> Api<L, M>
where
    L: EventLog + Send + Sync + 'static,
    M: SkillStore + Clone + Send + Sync + 'static,
{
    pub(super) async fn status(State(api): State<Arc<Self>>) -> Reply<Status> {
        let service = api.service().await?;
        let events = service.event_count().await?;
        Ok(Json(Data { data: Status { events } }))
    }

    pub(super) async fn projects(State(api): State<Arc<Self>>) -> Reply<Vec<ProjectSummary>> {
        let service = api.service().await?;
        let projects = service.projects().await?;
        let projects = projects.into_iter().map(|p| ProjectSummary::new(p, &api.home)).collect();
        Ok(Json(Data { data: projects }))
    }

    pub(super) async fn create_project(State(api): State<Arc<Self>>, Json(body): Json<NewProject>) -> Reply<ProjectSummary> {
        let name = Name::new(&body.name).map_err(|error| ApiError::BadRequest(error.to_string()))?;
        let service = api.service().await?;
        let project = service.create_project(Source::Web, Actor::Human, name, None).await?;
        Ok(Json(Data { data: ProjectSummary::new(project, &api.home) }))
    }

    pub(super) async fn sessions(State(api): State<Arc<Self>>) -> Reply<Vec<responses::Session>> {
        let service = api.service().await?;
        let sessions = service.sessions().await?;
        let projects = service.projects().await?;
        let sessions = sessions.into_iter().map(|s| responses::Session::new(s, &projects)).collect();
        Ok(Json(Data { data: sessions }))
    }

    pub(super) async fn session(State(api): State<Arc<Self>>, Path(id): Path<SessionId>) -> Reply<responses::Session> {
        let service = api.service().await?;
        let session = service.session(id).await?.ok_or(ApiError::NotFound)?;
        let projects = service.projects().await?;
        Ok(Json(Data { data: responses::Session::new(session, &projects) }))
    }

    pub(super) async fn maps(State(api): State<Arc<Self>>) -> Reply<Vec<MapSummary>> {
        let service = api.service().await?;
        let maps = service.all_maps().await?;
        let projects = service.projects().await?;
        let maps = maps.into_iter().map(|m| MapSummary::new(m, &projects)).collect();
        Ok(Json(Data { data: maps }))
    }

    pub(super) async fn map(State(api): State<Arc<Self>>, Path((project, name)): Path<(String, String)>) -> Reply<MapDetail> {
        let service = api.service().await?;
        let map = service.map_id(&project, &name).await?.ok_or(ApiError::NotFound)?;
        let detail = service.map_detail(map).await?.ok_or(ApiError::NotFound)?;
        Ok(Json(Data { data: MapDetail::new(detail, project) }))
    }

    pub(super) async fn boards(State(api): State<Arc<Self>>) -> Reply<Vec<BoardSummary>> {
        let service = api.service().await?;
        let boards = service.boards().await?;
        let projects = service.projects().await?;
        Ok(Json(Data { data: BoardSummary::all(boards, &projects) }))
    }

    pub(super) async fn board(State(api): State<Arc<Self>>, Path((project, board)): Path<(String, String)>) -> Reply<BoardDetail> {
        let service = api.service().await?;
        let contents = service.board_contents(&project, &board).await?.ok_or(ApiError::NotFound)?;
        Ok(Json(Data { data: BoardDetail::new(contents) }))
    }

    pub(super) async fn place_map(
        State(api): State<Arc<Self>>,
        Path((project, board)): Path<(String, String)>,
        Json(body): Json<PlacedMap>,
    ) -> Reply<BoardDetail> {
        let service = api.service().await?;
        let shown = service.board_contents(&project, &board).await?.ok_or(ApiError::NotFound)?;
        let map = service.map_id(shown.project.name.as_str(), &body.map).await?.ok_or(ApiError::NotFound)?;
        service.place_map(Source::Web, Actor::Human, map, Place::Board(shown.board.id)).await?;

        let contents = service.board_contents(&project, &board).await?.ok_or(ApiError::NotFound)?;
        Ok(Json(Data { data: BoardDetail::new(contents) }))
    }

    pub(super) async fn unplace_map(State(api): State<Arc<Self>>, Path((project, board, map)): Path<(String, String, String)>) -> Reply<BoardDetail> {
        let service = api.service().await?;
        let shown = service.board_contents(&project, &board).await?.ok_or(ApiError::NotFound)?;
        let map = service.map_id(shown.project.name.as_str(), &map).await?.ok_or(ApiError::NotFound)?;
        service.unplace_map(Source::Web, Actor::Human, map, Place::Board(shown.board.id)).await?;

        let contents = service.board_contents(&project, &board).await?.ok_or(ApiError::NotFound)?;
        Ok(Json(Data { data: BoardDetail::new(contents) }))
    }

    pub(super) async fn create_board(State(api): State<Arc<Self>>, Path(project): Path<String>) -> Reply<BoardSummary> {
        let service = api.service().await?;
        let board = service.create_board(Source::Web, Actor::Human, &project).await?.ok_or(ApiError::NotFound)?;
        Ok(Json(Data { data: BoardSummary::new(board, project) }))
    }

    pub(super) async fn session_events(State(api): State<Arc<Self>>, Path(id): Path<SessionId>) -> Reply<Vec<responses::Event>> {
        let service = api.service().await?;
        let events = service.session_events(id).await?.ok_or(ApiError::NotFound)?;
        let events = events.into_iter().map(|e| responses::Event::new(e, &api.home)).collect();
        Ok(Json(Data { data: events }))
    }
}
