use std::collections::BTreeMap;
use std::path::Path as FsPath;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use serde_json::json;

use super::Api;
use crate::core::{Actor, CITES, CitationSource, ClientSessionId, EventId, EventStore, SchemaStore, MapId, NodeBody, NodeId, Payload, Session, SessionId};

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
    let sessions: Vec<SessionView> = sessions.into_iter().map(|s| SessionView::new(s, &api.home)).collect();
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
    let session = SessionView::new(session, &api.home);
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

    let maps: Vec<MapView> = summaries
        .into_iter()
        .map(|s| MapView {
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

    let citations: BTreeMap<NodeId, CitationView> = detail
        .nodes
        .iter()
        .filter_map(|n| match &n.body {
            NodeBody::Citation(source) => Some((n.id, CitationView::new(source))),
            NodeBody::Note { .. } => None,
        })
        .collect();

    let map = MapDetailView {
        id: detail.map.id,
        kind: detail.map.kind,
        path: display_path(&detail.map.path, &api.home),
        nodes: detail
            .nodes
            .into_iter()
            .filter_map(|n| match n.body {
                NodeBody::Note { kind, title, properties } => {
                    let citations = detail
                        .edges
                        .iter()
                        .filter(|e| e.kind == CITES && e.from == n.id)
                        .filter_map(|e| citations.get(&e.to).cloned())
                        .collect();

                    Some(NodeView { id: n.id, kind, title, properties, citations, created_at: n.created_at })
                }
                NodeBody::Citation(_) => None,
            })
            .collect(),
        edges: detail
            .edges
            .into_iter()
            .filter(|e| e.kind != CITES)
            .map(|e| EdgeView { kind: e.kind, from: e.from, to: e.to })
            .collect(),
    };

    Ok(Json(json!({ "data": map })))
}

/// A map as the page reads it, with its path as a human sees it.
#[derive(Serialize)]
struct MapView {
    id: MapId,
    kind: String,
    path: String,
    nodes: usize,
}

/// A whole map as the page reads it, with its path as a human sees it.
#[derive(Serialize)]
struct MapDetailView {
    id: MapId,
    kind: String,
    path: String,
    nodes: Vec<NodeView>,
    edges: Vec<EdgeView>,
}

#[derive(Serialize)]
struct NodeView {
    id: NodeId,
    kind: String,
    title: String,
    properties: BTreeMap<String, String>,
    citations: Vec<CitationView>,
    created_at: jiff::Timestamp,
}

/// A citation as the page reads it: the snapshot from the log, with no pointer back to the source.
#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
enum CitationView {
    Message { quote: String },
    File { path: String, lines: (u32, u32), excerpt: String },
}

impl CitationView {
    fn new(source: &CitationSource) -> Self {
        match source {
            CitationSource::Message { quote, .. } => Self::Message { quote: quote.clone() },
            CitationSource::File { path, lines, excerpt } => Self::File { path: path.display().to_string(), lines: *lines, excerpt: excerpt.clone() },
        }
    }
}

#[derive(Serialize)]
struct EdgeView {
    kind: String,
    from: NodeId,
    to: NodeId,
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

pub(super) async fn session_events<S: EventStore + Send + Sync + 'static, M: SchemaStore + Send + Sync + 'static>(
    State(api): State<Arc<Api<S, M>>>,
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
        Payload::SubagentStarted { subagent, agent, task, .. } => {
            json!({ "subagent": subagent.as_str(), "agent": agent, "task": task })
        }
        Payload::SubagentReported { subagent, content, .. } => {
            json!({ "subagent": subagent.as_str(), "content": content })
        }
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
