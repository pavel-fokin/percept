use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

use super::paths::display_path;
use super::responses::{self, view_payload, Edge, MapDetail, MapSummary, Node, Reference};
use super::Api;
use crate::core::{EventStore, MapId, SchemaStore, SessionId};

pub(super) async fn status<S: EventStore + Send + Sync + 'static, M: SchemaStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S, M>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let events = api
        .service
        .event_count()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "data": { "events": events } })))
}

pub(super) async fn sessions<S: EventStore + Send + Sync + 'static, M: SchemaStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S, M>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sessions = api
        .service
        .sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let sessions: Vec<responses::Session> = sessions.into_iter().map(|s| responses::Session::new(s, &api.home)).collect();
    Ok(Json(json!({ "data": sessions })))
}

pub(super) async fn session<S: EventStore + Send + Sync + 'static, M: SchemaStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S, M>>>,
    Path(id): Path<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session = api
        .service
        .session(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let session = responses::Session::new(session, &api.home);
    Ok(Json(json!({ "data": session })))
}

pub(super) async fn maps<S: EventStore + Send + Sync + 'static, M: SchemaStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S, M>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let summaries = api
        .service
        .all_map_summaries()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let maps: Vec<MapSummary> = summaries
        .into_iter()
        .map(|s| MapSummary {
            id: s.map.id,
            kind: s.map.kind,
            path: display_path(&s.map.path, &api.home),
            nodes: s.nodes,
        })
        .collect();

    Ok(Json(json!({ "data": maps })))
}

pub(super) async fn map<S: EventStore + Send + Sync + 'static, M: SchemaStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S, M>>>,
    Path(id): Path<MapId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let detail = api
        .service
        .map_detail(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

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

    Ok(Json(json!({ "data": map })))
}

pub(super) async fn session_events<S: EventStore + Send + Sync + 'static, M: SchemaStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S, M>>>,
    Path(id): Path<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let events: Vec<responses::Event> = api
        .service
        .session_events(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?
        .into_iter()
        .map(|e| responses::Event {
            id: e.id,
            actor: e.actor,
            kind: e.payload.kind().name(),
            payload: view_payload(&e.payload, &api.home),
            created_at: e.created_at,
        })
        .collect();
    Ok(Json(json!({ "data": events })))
}
