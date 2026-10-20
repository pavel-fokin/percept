use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::paths::display_path;
use super::responses::{self, view_payload, Data, Edge, MapDetail, MapSummary, Node, Reference, Status};
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
        let summaries = api.service.all_map_summaries().await?;

        let maps = summaries
            .into_iter()
            .map(|s| MapSummary {
                id: s.map.id,
                kind: s.map.kind,
                path: display_path(&s.map.path, &api.home),
                nodes: s.nodes,
            })
            .collect();

        Ok(Json(Data { data: maps }))
    }

    pub(super) async fn map(State(api): State<Arc<Self>>, Path(id): Path<MapId>) -> Reply<MapDetail> {
        let detail = api.service.map_detail(id).await?.ok_or(ApiError::NotFound)?;

        let map = MapDetail {
            id: detail.map.id,
            kind: detail.map.kind,
            path: display_path(&detail.map.path, &api.home),
            nodes: detail
                .nodes
                .into_iter()
                .map(|n| Node {
                    id: n.id,
                    kind: n.kind,
                    title: n.title,
                    properties: n.properties,
                    references: n.references.into_iter().map(|r| Reference::new(r, &detail.messages)).collect(),
                    created_at: n.created_at,
                })
                .collect(),
            edges: detail.edges.into_iter().map(|e| Edge { kind: e.kind, from: e.from, to: e.to }).collect(),
        };

        Ok(Json(Data { data: map }))
    }

    pub(super) async fn session_events(State(api): State<Arc<Self>>, Path(id): Path<SessionId>) -> Reply<Vec<responses::Event>> {
        let events = api
            .service
            .session_events(id)
            .await?
            .ok_or(ApiError::NotFound)?
            .into_iter()
            .map(|e| responses::Event {
                id: e.id,
                actor: e.actor,
                kind: e.payload.kind().name(),
                payload: view_payload(&e.payload, &api.home),
                created_at: e.created_at,
            })
            .collect();
        Ok(Json(Data { data: events }))
    }
}
