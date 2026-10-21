//! Server layer: serves the embedded page and its JSON API over HTTP.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use axum::http::{header::CONTENT_TYPE, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;

use crate::app::AppService;
use crate::core::{EventStore, SchemaStore};
use crate::shared::Result;

mod handlers;
mod paths;
mod responses;

const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));
const FAVICON_SVG: &[u8] = include_bytes!("../../web/public/favicon.svg");
const FAVICON_ICO: &[u8] = include_bytes!("../../web/public/favicon.ico");
const APPLE_TOUCH_ICON: &[u8] = include_bytes!("../../web/public/apple-touch-icon.png");

pub async fn serve<S, M>(service: Arc<AppService<S, M>>, home: PathBuf, port: u16, open: bool) -> Result<()>
where
    S: EventStore + Send + Sync + 'static,
    M: SchemaStore + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let url = format!("http://{}", listener.local_addr()?);
    println!("percept • {url}");

    if open {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let _ = Command::new(opener).arg(&url).spawn();
    }

    axum::serve(listener, router(service, home)).await?;
    Ok(())
}

/// What the handlers read: the use cases, and the home that paths are shown under.
struct Api<S: EventStore, M: SchemaStore> {
    service: Arc<AppService<S, M>>,
    home: PathBuf,
}

fn router<S, M>(service: Arc<AppService<S, M>>, home: PathBuf) -> Router
where
    S: EventStore + Send + Sync + 'static,
    M: SchemaStore + Send + Sync + 'static,
{
    Router::new()
        .route("/api/status", get(Api::<S, M>::status))
        .route("/api/projects", get(Api::<S, M>::projects))
        .route("/api/sessions", get(Api::<S, M>::sessions))
        .route("/api/sessions/{id}", get(Api::<S, M>::session))
        .route("/api/sessions/{id}/events", get(Api::<S, M>::session_events))
        .route("/api/maps", get(Api::<S, M>::maps))
        .route("/api/projects/{project}/maps/{kind}", get(Api::<S, M>::map))
        .route("/favicon.svg", get(|| async { asset("image/svg+xml", FAVICON_SVG) }))
        .route("/favicon.ico", get(|| async { asset("image/x-icon", FAVICON_ICO) }))
        .route("/apple-touch-icon.png", get(|| async { asset("image/png", APPLE_TOUCH_ICON) }))
        .fallback(fallback)
        .with_state(Arc::new(Api { service, home }))
}

fn asset(content_type: &'static str, body: &'static [u8]) -> impl IntoResponse {
    ([(CONTENT_TYPE, content_type)], body)
}

async fn fallback(uri: Uri) -> Response {
    let path = uri.path();
    if path == "/api" || path.starts_with("/api/") {
        StatusCode::NOT_FOUND.into_response()
    } else {
        Html(PAGE).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use crate::core::{Actor, ClientSessionId, Event, MapId, NodeId, ProjectId, ProjectName, Reference, Schema, SchemaStore, SessionId, Source};

    const SESSION: &str = "0190a0a0-0000-7000-8000-000000000001";
    const MAP: &str = "0190a0a0-0000-7000-8000-000000000003";
    const PROJECT: &str = "0190a0a0-0000-7000-8000-000000000004";

    struct Fixed(usize, Vec<Reference>, Vec<Event>);

    impl EventStore for Fixed {
        type Lock = ();

        async fn lock(&self) -> Result<()> {
            unimplemented!()
        }

        async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>> {
            unimplemented!()
        }

        async fn all(&self) -> Result<Vec<Event>> {
            let session: SessionId = serde_json::from_value(json!(SESSION))?;
            let project: ProjectId = serde_json::from_value(json!(PROJECT))?;
            let project_created = Event::project_created(Source { client: "claude-code".into(), path: "/Users/me/repo".into() }, Actor::Human, project, ProjectName::new("repo").unwrap());
            let created = Event::session_created(Source::fixture(), session, project, ClientSessionId::new("k".into()));
            let started = (1..self.0).map(move |_| Event::session_started(Source::fixture(), session, json!({})));
            let map: MapId = serde_json::from_value(json!(MAP))?;
            let map_created = Event::map_created(Source::fixture(), Actor::Human, map, project, "decisions");
            let node = Event::node_added(Source::fixture(), Actor::Human, map, NodeId::new(), "question", "Q", Default::default(), self.1.clone());
            Ok([project_created, created].into_iter().chain(started).chain(self.2.clone()).chain([map_created, node]).collect())
        }

        async fn append(&self, _: &Event) -> Result<()> {
            unimplemented!()
        }
    }

    struct Schemas;

    impl SchemaStore for Schemas {
        async fn schema(&self, _: &std::path::Path, _: &str) -> Result<Schema> {
            unimplemented!()
        }
    }

    fn json_body(response: &str) -> serde_json::Value {
        serde_json::from_str(response.rsplit("\r\n\r\n").next().unwrap()).unwrap()
    }

    async fn get_response(path: &str) -> String {
        get_response_with_references(path, Vec::new(), Vec::new()).await
    }

    async fn get_response_with_references(path: &str, references: Vec<Reference>, messages: Vec<Event>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(Arc::new(AppService::new(Fixed(3, references, messages), Schemas)), "/Users/me".into());
        tokio::spawn(async move { axum::serve(listener, app).await });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn status_returns_the_event_count() {
        let response = get_response("/api/status").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.ends_with(r#"{"data":{"events":6}}"#));
    }

    #[tokio::test]
    async fn projects_returns_each_name_with_its_root_from_home() {
        let response = get_response("/api/projects").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        assert_eq!(json_body(&response)["data"], json!([{ "name": "repo", "root": "~/repo" }]));
    }

    #[tokio::test]
    async fn sessions_returns_id_and_client_session_id_per_session() {
        let response = get_response("/api/sessions").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        let body = json_body(&response);
        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["client_session_id"], "k");
        assert_eq!(data[0].as_object().unwrap().len(), 8);
        assert_eq!(data[0]["status"], "waiting");
        assert!(data[0]["last_event_at"].is_string());
        assert_eq!(data[0]["project"], "repo");
        assert_eq!(data[0]["client"], "claude-code");
        assert!(data[0]["id"].is_string());
    }

    #[tokio::test]
    async fn maps_returns_every_map_with_its_project_and_node_count() {
        let response = get_response("/api/maps").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        let body = json_body(&response);
        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].as_object().unwrap().len(), 3);
        assert_eq!(data[0]["kind"], "decisions");
        assert_eq!(data[0]["project"], "repo");
        assert_eq!(data[0]["nodes"], 1);
    }

    #[tokio::test]
    async fn map_returns_the_whole_map() {
        let response = get_response("/api/projects/repo/maps/decisions").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        let data = &json_body(&response)["data"];
        assert_eq!(data["kind"], "decisions");
        assert_eq!(data["project"], "repo");
        assert_eq!(data["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(data["nodes"][0]["title"], "Q");
        assert_eq!(data["nodes"][0]["properties"], json!({}));
        assert_eq!(data["nodes"][0]["references"], json!([]));
        assert!(data["nodes"][0]["created_at"].is_string());
        assert_eq!(data["nodes"][0]["changed_at"], json!(null));
        assert_eq!(data["edges"], json!([]));
    }

    #[tokio::test]
    async fn map_nodes_include_saved_message_and_file_references_in_order() {
        let session: SessionId = serde_json::from_value(json!(SESSION)).unwrap();
        let message = Event::message(Source::fixture(), session, Actor::Human, "A <quote>\n  **verbatim**", json!({}));
        let event = message.id;
        let references = vec![
            Reference::Message { event, quote: "A <quote>\n  **verbatim**".into() },
            Reference::File { path: "src/main.rs".into(), lines: (2, 4), excerpt: "fn main() {\n}\n".into() },
            Reference::Message { event, quote: "A <quote>\n  **verbatim**".into() },
        ];

        let response = get_response_with_references("/api/projects/repo/maps/decisions", references, vec![message.clone()]).await;

        assert!(response.starts_with("HTTP/1.1 200"));
        let data = &json_body(&response)["data"];
        assert_eq!(data["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(data["nodes"][0]["references"], json!([
            { "message": { "event": event, "session": session, "created_at": message.created_at, "quote": "A <quote>\n  **verbatim**" } },
            { "file": { "path": "src/main.rs", "lines": [2, 4], "excerpt": "fn main() {\n}\n" } },
            { "message": { "event": event, "session": session, "created_at": message.created_at, "quote": "A <quote>\n  **verbatim**" } },
        ]));
        assert_eq!(data["edges"], json!([]));
    }

    #[tokio::test]
    async fn unknown_map_is_not_found() {
        assert!(get_response("/api/projects/repo/maps/tasks").await.starts_with("HTTP/1.1 404"));
        assert!(get_response("/api/projects/other/maps/decisions").await.starts_with("HTTP/1.1 404"));
    }

    #[tokio::test]
    async fn session_returns_one_session() {
        let response = get_response(&format!("/api/sessions/{SESSION}")).await;

        assert!(response.starts_with("HTTP/1.1 200"));
        let body = json_body(&response);
        assert_eq!(body["data"]["id"], SESSION);
        assert_eq!(body["data"]["client_session_id"], "k");
        assert_eq!(body["data"]["client"], "claude-code");
        assert_eq!(body["data"]["project"], "repo");
    }

    #[tokio::test]
    async fn session_events_omit_raw() {
        let response = get_response(&format!("/api/sessions/{SESSION}/events")).await;

        assert!(response.starts_with("HTTP/1.1 200"));
        let body = json_body(&response);
        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 3);
        assert_eq!(data[0]["kind"], "session.created");
        assert_eq!(data[1]["kind"], "session.started");
        assert_eq!(data[1]["actor"], "system");
        let mut fields: Vec<&String> = data[1].as_object().unwrap().keys().collect();
        fields.sort();
        assert_eq!(fields, ["actor", "created_at", "id", "kind", "payload"]);
    }

    #[tokio::test]
    async fn unknown_session_is_not_found() {
        let unknown = "0190a0a0-0000-7000-8000-000000000002";

        assert!(get_response(&format!("/api/sessions/{unknown}")).await.starts_with("HTTP/1.1 404"));
        assert!(get_response(&format!("/api/sessions/{unknown}/events")).await.starts_with("HTTP/1.1 404"));
    }

    #[tokio::test]
    async fn other_paths_serve_the_page() {
        let response = get_response("/anything/here").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.ends_with(PAGE));
    }

    #[tokio::test]
    async fn unknown_api_paths_are_not_found() {
        assert!(get_response("/api/nope").await.starts_with("HTTP/1.1 404"));
        assert!(get_response("/api").await.starts_with("HTTP/1.1 404"));
    }
}
