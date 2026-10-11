use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use serde_json::json;

use crate::app::AppService;
use crate::core::{Actor, EventId, EventStore, Payload, SessionId};

pub(super) async fn status<S: EventStore + Send + Sync + 'static>(
    State(service): State<Arc<AppService<S>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let events = service
        .event_count()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "data": { "events": events } })))
}

pub(super) async fn sessions<S: EventStore + Send + Sync + 'static>(
    State(service): State<Arc<AppService<S>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sessions = service
        .sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "data": sessions })))
}

pub(super) async fn session<S: EventStore + Send + Sync + 'static>(
    State(service): State<Arc<AppService<S>>>,
    Path(id): Path<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session = service
        .session(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(json!({ "data": session })))
}

/// An event as the page reads it: the hook input stays in the log.
#[derive(Serialize)]
struct EventView {
    id: EventId,
    actor: Actor,
    kind: &'static str,
    payload: serde_json::Value,
    created_at: jiff::Timestamp,
}

pub(super) async fn session_events<S: EventStore + Send + Sync + 'static>(
    State(service): State<Arc<AppService<S>>>,
    Path(id): Path<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let events: Vec<EventView> = service
        .session_events(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?
        .into_iter()
        .map(|e| EventView {
            id: e.id,
            actor: e.actor,
            kind: e.payload.kind(),
            payload: view_payload(&e.payload),
            created_at: e.created_at,
        })
        .collect();
    Ok(Json(json!({ "data": events })))
}

fn view_payload(payload: &Payload) -> serde_json::Value {
    match payload {
        Payload::Message { content, .. } => json!({ "content": content }),
        Payload::ToolUsed { tool, .. } => json!({ "tool": tool }),
        Payload::SessionCreated { .. } | Payload::SessionStarted { .. } | Payload::SessionStopped { .. } => json!({}),
    }
}
