use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::responses::{self, Data, MapDetail, MapSummary, Status};
use super::Api;
use crate::core::{EventStore, MapId, SchemaStore, SessionId};
use crate::shared::Error;

type Reply<T> = Result<Json<Data<T>>, ApiError>;

pub(super) enum ApiError {
    NotFound,
    Internal(Error),
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

    pub(super) async fn sessions(State(api): State<Arc<Self>>) -> Reply<Vec<responses::Session>> {
        let sessions = api.service.sessions().await?;
        let sessions = sessions.into_iter().map(|s| responses::Session::new(s, &api.home)).collect();
        Ok(Json(Data { data: sessions }))
    }

    pub(super) async fn session(State(api): State<Arc<Self>>, Path(id): Path<SessionId>) -> Reply<responses::Session> {
        let session = api.service.session(id).await?.ok_or(ApiError::NotFound)?;
        Ok(Json(Data { data: responses::Session::new(session, &api.home) }))
    }

    pub(super) async fn maps(State(api): State<Arc<Self>>) -> Reply<Vec<MapSummary>> {
        let maps = api.service.all_maps().await?;
        let maps = maps.into_iter().map(|m| MapSummary::new(m, &api.home)).collect();
        Ok(Json(Data { data: maps }))
    }

    pub(super) async fn map(State(api): State<Arc<Self>>, Path(id): Path<MapId>) -> Reply<MapDetail> {
        let detail = api.service.map_detail(id).await?.ok_or(ApiError::NotFound)?;
        Ok(Json(Data { data: MapDetail::new(detail, &api.home) }))
    }

    pub(super) async fn session_events(State(api): State<Arc<Self>>, Path(id): Path<SessionId>) -> Reply<Vec<responses::Event>> {
        let events = api.service.session_events(id).await?.ok_or(ApiError::NotFound)?;
        let events = events.into_iter().map(|e| responses::Event::new(e, &api.home)).collect();
        Ok(Json(Data { data: events }))
    }
}
