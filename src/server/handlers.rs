use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::responses::{self, BoardSummary, Data, MapDetail, MapSummary, ProjectSummary, Status};
use super::Api;
use crate::app::CreateBoardError;
use crate::core::{Actor, EventStore, SchemaStore, SessionId};
use crate::shared::Error;

type Reply<T> = Result<Json<Data<T>>, ApiError>;

pub(super) enum ApiError {
    NotFound,
    /// The reason, written as plain text for the page to show.
    BadRequest(String),
    Internal(Error),
}

impl From<CreateBoardError> for ApiError {
    fn from(error: CreateBoardError) -> Self {
        match error {
            CreateBoardError::UnknownProject => Self::NotFound,
            CreateBoardError::InvalidName(_) | CreateBoardError::NameTaken(_) => Self::BadRequest(error.to_string()),
            CreateBoardError::Store(error) => Self::Internal(error),
        }
    }
}

#[derive(Deserialize)]
pub(super) struct NewBoard {
    name: String,
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        Self::Internal(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::BadRequest(reason) => (StatusCode::BAD_REQUEST, reason).into_response(),
            Self::Internal(error) => {
                eprintln!("percept: {error}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

impl<S, M> Api<S, M>
where
    S: EventStore + Send + Sync + 'static,
    M: SchemaStore + Send + Sync + 'static,
{
    pub(super) async fn status(State(api): State<Arc<Self>>) -> Reply<Status> {
        let events = api.service.event_count().await?;
        Ok(Json(Data { data: Status { events } }))
    }

    pub(super) async fn projects(State(api): State<Arc<Self>>) -> Reply<Vec<ProjectSummary>> {
        let projects = api.service.projects().await?;
        let projects = projects.into_iter().map(|p| ProjectSummary::new(p, &api.home)).collect();
        Ok(Json(Data { data: projects }))
    }

    pub(super) async fn sessions(State(api): State<Arc<Self>>) -> Reply<Vec<responses::Session>> {
        let sessions = api.service.sessions().await?;
        let projects = api.service.projects().await?;
        let sessions = sessions.into_iter().map(|s| responses::Session::new(s, &projects)).collect();
        Ok(Json(Data { data: sessions }))
    }

    pub(super) async fn session(State(api): State<Arc<Self>>, Path(id): Path<SessionId>) -> Reply<responses::Session> {
        let session = api.service.session(id).await?.ok_or(ApiError::NotFound)?;
        let projects = api.service.projects().await?;
        Ok(Json(Data { data: responses::Session::new(session, &projects) }))
    }

    pub(super) async fn maps(State(api): State<Arc<Self>>) -> Reply<Vec<MapSummary>> {
        let maps = api.service.all_maps().await?;
        let projects = api.service.projects().await?;
        let maps = maps.into_iter().map(|m| MapSummary::new(m, &projects)).collect();
        Ok(Json(Data { data: maps }))
    }

    pub(super) async fn map(State(api): State<Arc<Self>>, Path((project, kind)): Path<(String, String)>) -> Reply<MapDetail> {
        let detail = api.service.map_detail(&project, &kind).await?.ok_or(ApiError::NotFound)?;
        Ok(Json(Data { data: MapDetail::new(detail, project) }))
    }

    pub(super) async fn boards(State(api): State<Arc<Self>>) -> Reply<Vec<BoardSummary>> {
        let boards = api.service.boards().await?;
        let projects = api.service.projects().await?;
        let boards = boards.into_iter().map(|b| BoardSummary::new(b, &projects)).collect();
        Ok(Json(Data { data: boards }))
    }

    pub(super) async fn board(State(api): State<Arc<Self>>, Path((project, board)): Path<(String, String)>) -> Reply<BoardSummary> {
        let board = api.service.board(&project, &board).await?.ok_or(ApiError::NotFound)?;
        let projects = api.service.projects().await?;
        Ok(Json(Data { data: BoardSummary::new(board, &projects) }))
    }

    pub(super) async fn create_board(State(api): State<Arc<Self>>, Path(project): Path<String>, Json(new): Json<NewBoard>) -> Reply<BoardSummary> {
        let board = api.service.create_board("percept-web", Actor::Human, &project, &new.name).await?;
        let projects = api.service.projects().await?;
        Ok(Json(Data { data: BoardSummary::new(board, &projects) }))
    }

    pub(super) async fn session_events(State(api): State<Arc<Self>>, Path(id): Path<SessionId>) -> Reply<Vec<responses::Event>> {
        let events = api.service.session_events(id).await?.ok_or(ApiError::NotFound)?;
        let events = events.into_iter().map(|e| responses::Event::new(e, &api.home)).collect();
        Ok(Json(Data { data: events }))
    }
}
