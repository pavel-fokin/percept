use std::path::Path as FsPath;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use serde_json::json;

use super::Api;
use crate::core::{Actor, ClientSessionId, EventId, EventStore, Payload, Session, SessionId};

pub(super) async fn status<S: EventStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let events = api
        .service
        .event_count()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "data": { "events": events } })))
}

pub(super) async fn sessions<S: EventStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sessions = api
        .service
        .sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let sessions: Vec<SessionView> = sessions.into_iter().map(|s| SessionView::new(s, &api.home)).collect();
    Ok(Json(json!({ "data": sessions })))
}

pub(super) async fn session<S: EventStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session = api
        .service
        .session(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let session = SessionView::new(session, &api.home);
    Ok(Json(json!({ "data": session })))
}

/// A session as the page reads it, with its path as a human sees it.
#[derive(Serialize)]
struct SessionView {
    id: SessionId,
    client_session_id: ClientSessionId,
    created_at: jiff::Timestamp,
    title: Option<String>,
    path: String,
}

impl SessionView {
    fn new(session: Session, home: &FsPath) -> Self {
        Self {
            id: session.id,
            client_session_id: session.client_session_id,
            created_at: session.created_at,
            title: session.title,
            path: display_path(&session.path, home),
        }
    }
}

/// `~/rest` under home, the absolute path otherwise. A home of `/` or empty is no home: everything would sit under it.
fn display_path(path: &FsPath, home: &FsPath) -> String {
    let rest = home.parent().and_then(|_| path.strip_prefix(home).ok());

    match rest {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
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
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let events: Vec<EventView> = api
        .service
        .session_events(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?
        .into_iter()
        .map(|e| EventView {
            id: e.id,
            actor: e.actor,
            kind: e.payload.kind().name(),
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
        Payload::SessionCreated { .. }
        | Payload::SessionStarted { .. }
        | Payload::SessionStopped { .. }
        | Payload::MapCreated { .. }
        | Payload::NodeAdded { .. }
        | Payload::EdgeAdded { .. } => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/pavel-fokin";

    fn shown(path: &str) -> String {
        display_path(FsPath::new(path), FsPath::new(HOME))
    }

    #[test]
    fn home_itself_is_a_tilde() {
        assert_eq!(shown("/Users/pavel-fokin"), "~");
    }

    #[test]
    fn a_path_under_home_starts_with_a_tilde() {
        assert_eq!(shown("/Users/pavel-fokin/code/percept"), "~/code/percept");
    }

    #[test]
    fn a_path_outside_home_stays_absolute() {
        assert_eq!(shown("/work/app"), "/work/app");
    }

    #[test]
    fn a_sibling_of_home_is_outside_it() {
        assert_eq!(shown("/Users/pavel-fokinX/code"), "/Users/pavel-fokinX/code");
    }

    #[test]
    fn a_root_or_empty_home_is_no_home() {
        assert_eq!(display_path(FsPath::new("/work/app"), FsPath::new("/")), "/work/app");
        assert_eq!(display_path(FsPath::new("/work/app"), FsPath::new("")), "/work/app");
    }

}
