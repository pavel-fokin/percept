//! Server layer: serves the embedded page and its JSON API over HTTP.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use axum::http::{header::CONTENT_TYPE, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::Router;
use tokio::net::TcpListener;

use crate::app::AppService;
use crate::core::{EventStore, SkillStore};
use crate::shared::{Begin, Result};

mod handlers;
mod paths;
mod responses;

const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));
const FAVICON_SVG: &[u8] = include_bytes!("../../web/public/favicon.svg");
const FAVICON_ICO: &[u8] = include_bytes!("../../web/public/favicon.ico");
const APPLE_TOUCH_ICON: &[u8] = include_bytes!("../../web/public/apple-touch-icon.png");

pub async fn serve<L, M>(log: L, skills: M, home: PathBuf, port: u16, open: bool) -> Result<()>
where
    L: Begin<Transaction: EventStore + Send + Sync> + Send + Sync + 'static,
    M: SkillStore + Clone + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let url = format!("http://{}", listener.local_addr()?);
    println!("percept • {url}");

    if open {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let _ = Command::new(opener).arg(&url).spawn();
    }

    axum::serve(listener, router(log, skills, home)).await?;
    Ok(())
}

/// What the handlers read: the log and skills the use cases run over, and the home that paths are shown under.
struct Api<L: Begin<Transaction: EventStore + Send + Sync>, M: SkillStore> {
    log: L,
    skills: M,
    home: PathBuf,
}

impl<L: Begin<Transaction: EventStore + Send + Sync>, M: SkillStore + Clone> Api<L, M> {
    /// The use cases over a new transaction, one for each request.
    async fn service(&self) -> Result<AppService<L::Transaction, M>> {
        Ok(AppService::new(self.log.begin().await?, self.skills.clone()))
    }
}

fn router<L, M>(log: L, skills: M, home: PathBuf) -> Router
where
    L: Begin<Transaction: EventStore + Send + Sync> + Send + Sync + 'static,
    M: SkillStore + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/status", get(Api::<L, M>::status))
        .route("/api/projects", get(Api::<L, M>::projects).post(Api::<L, M>::create_project))
        .route("/api/sessions", get(Api::<L, M>::sessions))
        .route("/api/sessions/{id}", get(Api::<L, M>::session))
        .route("/api/sessions/{id}/events", get(Api::<L, M>::session_events))
        .route("/api/skills", get(Api::<L, M>::skills))
        .route("/api/maps", get(Api::<L, M>::maps))
        .route("/api/projects/{project}/maps/{name}", get(Api::<L, M>::map))
        .route("/api/projects/{project}/maps/{name}/nodes", post(Api::<L, M>::add_node))
        .route("/api/projects/{project}/maps/{name}/nodes/{node}", patch(Api::<L, M>::change_node).delete(Api::<L, M>::remove_node))
        .route("/api/projects/{project}/maps/{name}/edges", post(Api::<L, M>::add_edge).delete(Api::<L, M>::remove_edge))
        .route("/api/boards", get(Api::<L, M>::boards))
        .route("/api/projects/{project}/boards", post(Api::<L, M>::create_board))
        .route("/api/projects/{project}/boards/{board}", get(Api::<L, M>::board))
        .route("/api/projects/{project}/boards/{board}/maps", post(Api::<L, M>::place_map))
        .route("/api/projects/{project}/boards/{board}/maps/{map}", post(Api::<L, M>::create_map).delete(Api::<L, M>::unplace_map))
        .route("/favicon.svg", get(|| async { asset("image/svg+xml", FAVICON_SVG) }))
        .route("/favicon.ico", get(|| async { asset("image/x-icon", FAVICON_ICO) }))
        .route("/apple-touch-icon.png", get(|| async { asset("image/png", APPLE_TOUCH_ICON) }))
        .fallback(fallback)
        .with_state(Arc::new(Api { log, skills, home }))
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
    use std::collections::{BTreeMap, BTreeSet};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use crate::core::{Actor, BoardId, ClientSessionId, EdgeKind, Event, EventStore, MapId, Name, NodeId, Payload, Place, Places, Project, ProjectId, Reference, Schema, SessionId, Skill, SkillStore, Source};

    const SESSION: &str = "0190a0a0-0000-7000-8000-000000000001";
    const MAP: &str = "0190a0a0-0000-7000-8000-000000000003";
    const PROJECT: &str = "0190a0a0-0000-7000-8000-000000000004";

    #[derive(Clone)]
    struct Fixed(usize, Vec<Reference>, Vec<Event>);

    impl Begin for Fixed {
        type Transaction = Fixed;

        async fn begin(&self) -> Result<Fixed> {
            Ok(self.clone())
        }
    }

    impl EventStore for Fixed {
        async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>> {
            unimplemented!()
        }

        async fn all(&self) -> Result<Vec<Event>> {
            let session: SessionId = serde_json::from_value(json!(SESSION))?;
            let project: ProjectId = serde_json::from_value(json!(PROJECT))?;
            let project_created = Event::project_created(Source::Checkout { client: "claude-code".into(), path: "/Users/me/repo".into() }, Actor::Human, project, Name::new("repo").unwrap(), Some("/Users/me/repo".into()));
            let created = Event::session_created(Source::fixture(), session, project, ClientSessionId::new("k".into()));
            let started = (1..self.0).map(move |_| Event::session_started(Source::fixture(), session, json!({})));
            let map: MapId = serde_json::from_value(json!(MAP))?;
            let map_created = Event::map_created(Source::fixture(), Actor::Human, map, project, Name::new("decisions").unwrap(), Some("/maps/decisions.toml".into()), None, Places::fixture(project));
            let node = Event::node_added(Source::fixture(), Actor::Human, map, NodeId::new(), "question", "Q", Default::default(), self.1.clone());
            Ok([project_created, created].into_iter().chain(started).chain(self.2.clone()).chain([map_created, node]).collect())
        }

        async fn append(&self, _: &Event) -> Result<()> {
            unimplemented!()
        }
    }

    /// A log that takes appends, holding one project named `repo` rooted at `/Users/me/repo`.
    struct Log(std::sync::Mutex<Vec<Event>>);

    impl Log {
        fn new() -> Self {
            let source = Source::Checkout { client: "claude-code".into(), path: "/Users/me/repo".into() };
            let project = Event::project_created(source, Actor::Human, ProjectId::new(), Name::new("repo").unwrap(), Some("/Users/me/repo".into()));

            Self(std::sync::Mutex::new(vec![project]))
        }
    }

    impl Begin for Arc<Log> {
        type Transaction = Arc<Log>;

        async fn begin(&self) -> Result<Arc<Log>> {
            Ok(self.clone())
        }
    }

    impl EventStore for Arc<Log> {
        async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>> {
            unimplemented!()
        }

        async fn all(&self) -> Result<Vec<Event>> {
            Ok(self.0.lock().unwrap().clone())
        }

        async fn append(&self, event: &Event) -> Result<()> {
            self.0.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    #[derive(Clone)]
    struct Skills;

    impl SkillStore for Skills {
        async fn skill(&self, dir: &std::path::Path) -> Result<Skill> {
            assert_eq!(dir, std::path::Path::new("/Users/me/.percept/skills/plain"));

            Ok(Skill::new("plain".into(), "Anything.".into(), "Write.".into(), None))
        }

        async fn skills(&self, library: &std::path::Path) -> Result<Vec<(String, Result<Skill>)>> {
            assert_eq!(library, std::path::Path::new("/Users/me/.percept/skills"));
            let nodes = BTreeMap::from([("question".to_string(), BTreeSet::from(["status".to_string()]))]);
            let edges = BTreeMap::from([("answers".to_string(), EdgeKind { from: "question".into(), to: "question".into() })]);
            let schema = Schema::new(nodes, edges)?;

            Ok(vec![
                ("broken".into(), Err("/Users/me/.percept/skills/broken: no SKILL.md".into())),
                ("decision".into(), Ok(Skill::new("decision".into(), "Decide.".into(), "Do it.".into(), Some(schema)))),
                ("plain".into(), Ok(Skill::new("plain".into(), "Anything.".into(), "Write.".into(), None))),
            ])
        }
    }

    fn text_body(response: &str) -> &str {
        response.rsplit("\r\n\r\n").next().unwrap()
    }

    fn json_body(response: &str) -> serde_json::Value {
        serde_json::from_str(text_body(response)).unwrap()
    }

    async fn serve<L: Begin<Transaction: EventStore + Send + Sync> + Send + Sync + 'static>(log: L) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(log, Skills, "/Users/me".into());
        tokio::spawn(async move { axum::serve(listener, app).await });

        addr
    }

    async fn send(addr: std::net::SocketAddr, method: &str, path: &str, body: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = format!("{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    async fn get_response(path: &str) -> String {
        get_response_with_references(path, Vec::new(), Vec::new()).await
    }

    async fn get_response_with_references(path: &str, references: Vec<Reference>, messages: Vec<Event>) -> String {
        let addr = serve(Fixed(3, references, messages)).await;

        send(addr, "GET", path, "").await
    }

    #[tokio::test]
    async fn status_returns_the_event_count() {
        let response = get_response("/api/status").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.ends_with(r#"{"data":{"events":6}}"#));
    }

    #[tokio::test]
    async fn skills_returns_the_library_and_each_skill_or_its_error() {
        let response = get_response("/api/skills").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        assert_eq!(
            json_body(&response)["data"],
            json!({
                "library": "~/.percept/skills",
                "skills": [
                    { "name": "broken", "error": "~/.percept/skills/broken: no SKILL.md" },
                    {
                        "name": "decision", "description": "Decide.", "instructions": "Do it.",
                        "schema": { "nodes": { "question": ["status"] }, "edges": { "answers": { "from": "question", "to": "question" } } }
                    },
                    { "name": "plain", "description": "Anything.", "instructions": "Write.", "schema": null },
                ]
            })
        );
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
        assert_eq!(data[0]["name"], "decisions");
        assert_eq!(data[0]["project"], "repo");
        assert_eq!(data[0]["nodes"], 1);
    }

    #[tokio::test]
    async fn map_returns_the_whole_map() {
        let response = get_response("/api/projects/repo/maps/decisions").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        let data = &json_body(&response)["data"];
        assert_eq!(data["name"], "decisions");
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

    #[tokio::test]
    async fn a_posted_board_is_created_and_returned_then_read_back() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;

        let created = send(addr, "POST", "/api/projects/repo/boards", "").await;
        let id = json_body(&created)["data"]["id"].as_str().unwrap().to_string();
        let all = send(addr, "GET", "/api/boards", "").await;
        let by_name = send(addr, "GET", "/api/projects/repo/boards/board", "").await;
        let by_id = send(addr, "GET", &format!("/api/projects/repo/boards/{id}"), "").await;

        let board = json!({ "id": id, "name": "board", "project": "repo" });
        assert!(created.starts_with("HTTP/1.1 200"));
        assert_eq!(json_body(&created)["data"], board);
        assert_eq!(json_body(&all)["data"], json!([board]));
        assert_eq!(json_body(&by_name)["data"], json!({ "id": id, "name": "board", "project": "repo", "maps": [] }));
        assert!(by_id.starts_with("HTTP/1.1 404"));
        let event = log.0.lock().unwrap().last().unwrap().clone();
        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.source, Source::Web);
    }

    #[tokio::test]
    async fn a_posted_map_is_shown_on_the_board_with_its_nodes_and_edges() {
        let log = Arc::new(Log::new());
        let project = Project::all(&log.0.lock().unwrap()).remove(0).id;
        let map = MapId::new();
        let (a, b) = (NodeId::new(), NodeId::new());
        log.0.lock().unwrap().extend([
            Event::map_created(Source::fixture(), Actor::Human, map, project, Name::new("decisions").unwrap(), Some("/maps/decisions.toml".into()), None, Places::fixture(project)),
            Event::node_added(Source::fixture(), Actor::Human, map, a, "question", "Q", Default::default(), Vec::new()),
            Event::node_added(Source::fixture(), Actor::Human, map, b, "option", "O", Default::default(), Vec::new()),
            Event::edge_added(Source::fixture(), Actor::Human, map, "answers", b, a),
        ]);
        let addr = serve(log.clone()).await;
        send(addr, "POST", "/api/projects/repo/boards", "").await;

        let placed = send(addr, "POST", "/api/projects/repo/boards/board/maps", r#"{"map":"Decisions"}"#).await;
        let again = send(addr, "POST", "/api/projects/repo/boards/board/maps", r#"{"map":"decisions"}"#).await;
        let read = send(addr, "GET", "/api/projects/repo/boards/board", "").await;
        let unknown = send(addr, "POST", "/api/projects/repo/boards/board/maps", r#"{"map":"tasks"}"#).await;

        let maps = json!([{
            "name": "decisions",
            "nodes": [{ "id": a, "title": "Q" }, { "id": b, "title": "O" }],
            "edges": [{ "kind": "answers", "from": b, "to": a }],
        }]);
        assert!(placed.starts_with("HTTP/1.1 200"));
        assert_eq!(json_body(&placed)["data"]["maps"], maps);
        assert_eq!(json_body(&again)["data"]["maps"], maps);
        assert_eq!(json_body(&read)["data"]["maps"], maps);
        assert!(unknown.starts_with("HTTP/1.1 404"));
        let events = log.0.lock().unwrap();
        let changes = events.iter().filter(|e| matches!(e.payload, Payload::MapChanged { .. })).count();
        assert_eq!(changes, 1);
        assert_eq!(events.last().unwrap().source, Source::Web);
    }

    #[tokio::test]
    async fn a_deleted_map_leaves_the_board_unless_the_board_is_its_only_place() {
        let log = Arc::new(Log::new());
        let project = Project::all(&log.0.lock().unwrap()).remove(0).id;
        log.0.lock().unwrap().push(Event::map_created(Source::fixture(), Actor::Human, MapId::new(), project, Name::new("decisions").unwrap(), Some("/maps/decisions.toml".into()), None, Places::fixture(project)));
        let addr = serve(log.clone()).await;
        send(addr, "POST", "/api/projects/repo/boards", "").await;
        send(addr, "POST", "/api/projects/repo/boards/board/maps", r#"{"map":"decisions"}"#).await;

        let removed = send(addr, "DELETE", "/api/projects/repo/boards/board/maps/Decisions", "").await;
        let again = send(addr, "DELETE", "/api/projects/repo/boards/board/maps/decisions", "").await;
        let unknown = send(addr, "DELETE", "/api/projects/repo/boards/board/maps/tasks", "").await;

        assert!(removed.starts_with("HTTP/1.1 200"));
        assert_eq!(json_body(&removed)["data"]["maps"], json!([]));
        assert!(again.starts_with("HTTP/1.1 200"));
        assert!(unknown.starts_with("HTTP/1.1 404"));
        let changes = |log: &Log| log.0.lock().unwrap().iter().filter(|e| matches!(e.payload, Payload::MapChanged { .. })).count();
        assert_eq!(changes(&log), 2);
    }

    #[tokio::test]
    async fn a_deleted_map_shown_only_on_the_board_is_a_conflict() {
        let log = Arc::new(Log::new());
        let project = Project::all(&log.0.lock().unwrap()).remove(0).id;
        let map = MapId::new();
        log.0.lock().unwrap().push(Event::map_created(Source::fixture(), Actor::Human, map, project, Name::new("decisions").unwrap(), Some("/maps/decisions.toml".into()), None, Places::fixture(project)));
        let addr = serve(log.clone()).await;
        let created = send(addr, "POST", "/api/projects/repo/boards", "").await;
        let board: BoardId = serde_json::from_value(json_body(&created)["data"]["id"].clone()).unwrap();
        let only_here = Places::new([Place::Board(board)]).unwrap();
        log.0.lock().unwrap().push(Event::map_changed(Source::fixture(), Actor::Human, map, only_here));
        let written = log.0.lock().unwrap().len();

        let response = send(addr, "DELETE", "/api/projects/repo/boards/board/maps/decisions", "").await;

        assert!(response.starts_with("HTTP/1.1 409"));
        assert_eq!(text_body(&response), "this is the only place the map is shown");
        assert_eq!(log.0.lock().unwrap().len(), written);
    }

    #[tokio::test]
    async fn a_posted_map_is_created_on_the_board_with_or_without_a_skill() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;
        send(addr, "POST", "/api/projects/repo/boards", "").await;

        let bare = send(addr, "POST", "/api/projects/repo/boards/board/maps/Notes", r#"{"skill":null}"#).await;
        let skilled = send(addr, "POST", "/api/projects/repo/boards/board/maps/ideas", r#"{"skill":"plain"}"#).await;

        assert!(bare.starts_with("HTTP/1.1 200"));
        assert!(skilled.starts_with("HTTP/1.1 200"));
        let names: Vec<_> = json_body(&skilled)["data"]["maps"].as_array().unwrap().iter().map(|m| m["name"].clone()).collect();
        assert_eq!(names, [json!("notes"), json!("ideas")]);
        let events = log.0.lock().unwrap();
        let created: Vec<_> = events.iter().filter(|e| matches!(e.payload, Payload::MapCreated { .. })).collect();
        assert_eq!(created.len(), 2);
        assert!(created.iter().all(|e| e.source == Source::Web && e.actor == Actor::Human));
    }

    #[tokio::test]
    async fn a_posted_map_with_a_bad_or_taken_name_or_skill_is_refused() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;
        send(addr, "POST", "/api/projects/repo/boards", "").await;
        send(addr, "POST", "/api/projects/repo/boards/board/maps/notes", r#"{"skill":null}"#).await;
        let written = log.0.lock().unwrap().len();

        let invalid = send(addr, "POST", "/api/projects/repo/boards/board/maps/my%20map", r#"{"skill":null}"#).await;
        let taken = send(addr, "POST", "/api/projects/repo/boards/board/maps/Notes", r#"{"skill":null}"#).await;
        let path = send(addr, "POST", "/api/projects/repo/boards/board/maps/ideas", r#"{"skill":"../plain"}"#).await;
        let unknown = send(addr, "POST", "/api/projects/repo/boards/nope/maps/ideas", r#"{"skill":null}"#).await;

        assert!(invalid.starts_with("HTTP/1.1 400"));
        assert!(text_body(&invalid).starts_with("invalid name"));
        assert!(taken.starts_with("HTTP/1.1 409"));
        assert_eq!(text_body(&taken), "this project already has a notes map");
        assert!(path.starts_with("HTTP/1.1 400"));
        assert_eq!(text_body(&path), "\"../plain\" is not a skill name");
        assert!(unknown.starts_with("HTTP/1.1 404"));
        assert_eq!(log.0.lock().unwrap().len(), written);
    }

    #[tokio::test]
    async fn a_human_draws_nodes_and_edges_on_a_map_from_the_web() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;
        send(addr, "POST", "/api/projects/repo/boards", "").await;
        send(addr, "POST", "/api/projects/repo/boards/board/maps/notes", r#"{"skill":null}"#).await;
        let nodes = "/api/projects/repo/maps/notes/nodes";

        let a = json_body(&send(addr, "POST", nodes, r#"{"kind":"idea","title":"A"}"#).await)["data"]["nodes"][0]["id"].clone();
        let b = json_body(&send(addr, "POST", nodes, r#"{"kind":"idea","title":"B"}"#).await)["data"]["nodes"][0]["id"].clone();
        let (a, b) = (a.as_str().unwrap(), b.as_str().unwrap());
        let linked = send(addr, "POST", "/api/projects/repo/maps/notes/edges", &format!(r#"{{"from":"{a}","kind":"has","to":"{b}"}}"#)).await;
        let renamed = send(addr, "PATCH", &format!("{nodes}/{a}"), r#"{"title":"Root"}"#).await;
        let unlinked = send(addr, "DELETE", &format!("/api/projects/repo/maps/notes/edges?from={a}&kind=has&to={b}"), "").await;
        let removed = send(addr, "DELETE", &format!("{nodes}/{b}"), "").await;

        assert_eq!(json_body(&linked)["data"]["edges"], json!([{ "kind": "has", "from": a, "to": b }]));
        assert_eq!(json_body(&renamed)["data"]["nodes"][1]["title"], "Root");
        assert_eq!(json_body(&unlinked)["data"]["edges"], json!([]));
        let left: Vec<_> = json_body(&removed)["data"]["nodes"].as_array().unwrap().iter().map(|n| n["title"].clone()).collect();
        assert_eq!(left, [json!("Root")]);
        let events = log.0.lock().unwrap();
        let drawn: Vec<_> = events.iter().filter(|e| e.payload.map().is_some() && !matches!(e.payload, Payload::MapCreated { .. })).collect();
        assert_eq!(drawn.len(), 6);
        assert!(drawn.iter().all(|e| e.source == Source::Web && e.actor == Actor::Human));
    }

    #[tokio::test]
    async fn a_drawing_the_map_turns_down_is_a_bad_request_and_an_unknown_map_is_not_found() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;
        send(addr, "POST", "/api/projects/repo/boards", "").await;
        send(addr, "POST", "/api/projects/repo/boards/board/maps/notes", r#"{"skill":null}"#).await;
        send(addr, "POST", "/api/projects/repo/maps/notes/nodes", r#"{"kind":"idea","title":"A"}"#).await;
        let written = log.0.lock().unwrap().len();

        let blank = send(addr, "POST", "/api/projects/repo/maps/notes/nodes", r#"{"kind":"idea","title":" "}"#).await;
        let taken = send(addr, "POST", "/api/projects/repo/maps/notes/nodes", r#"{"kind":"idea","title":"A"}"#).await;
        let missing = send(addr, "DELETE", "/api/projects/repo/maps/notes/nodes/0190a0a0-0000-7000-8000-0000000000ff", "").await;
        let unknown = send(addr, "POST", "/api/projects/repo/maps/nope/nodes", r#"{"kind":"idea","title":"B"}"#).await;

        assert!(blank.starts_with("HTTP/1.1 400"));
        assert_eq!(text_body(&blank), "title must not be blank");
        assert!(taken.starts_with("HTTP/1.1 400"));
        assert_eq!(text_body(&taken), "the notes map already has a node titled \"A\"");
        assert!(missing.starts_with("HTTP/1.1 400"));
        assert!(unknown.starts_with("HTTP/1.1 404"));
        assert_eq!(log.0.lock().unwrap().len(), written);
    }

    #[tokio::test]
    async fn posted_boards_are_named_in_turn() {
        let addr = serve(Arc::new(Log::new())).await;

        let first = send(addr, "POST", "/api/projects/repo/boards", "").await;
        let second = send(addr, "POST", "/api/projects/repo/boards", "").await;

        assert_eq!(json_body(&first)["data"]["name"], "board");
        assert_eq!(json_body(&second)["data"]["name"], "board-1");
    }

    #[tokio::test]
    async fn an_unknown_project_or_board_is_not_found() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;
        let before = log.0.lock().unwrap().len();

        let post = send(addr, "POST", "/api/projects/other/boards", "").await;
        let no_board = send(addr, "GET", "/api/projects/repo/boards/board", "").await;
        let no_project = send(addr, "GET", "/api/projects/other/boards/ideas", "").await;

        assert!(post.starts_with("HTTP/1.1 404"));
        assert_eq!(log.0.lock().unwrap().len(), before);
        assert!(no_board.starts_with("HTTP/1.1 404"));
        assert!(no_project.starts_with("HTTP/1.1 404"));
    }

    #[tokio::test]
    async fn a_posted_project_is_created_without_a_root_and_listed() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;

        let created = send(addr, "POST", "/api/projects", r#"{"name":"Notes"}"#).await;
        let all = send(addr, "GET", "/api/projects", "").await;

        let notes = json!({ "name": "notes", "root": null });
        assert!(created.starts_with("HTTP/1.1 200"));
        assert_eq!(json_body(&created)["data"], notes);
        assert_eq!(json_body(&all)["data"], json!([{ "name": "repo", "root": "~/repo" }, notes]));
        let event = log.0.lock().unwrap().last().unwrap().clone();
        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.source, Source::Web);
    }

    #[tokio::test]
    async fn a_posted_project_with_a_taken_name_conflicts() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;

        let response = send(addr, "POST", "/api/projects", r#"{"name":"REPO"}"#).await;

        assert!(response.starts_with("HTTP/1.1 409"));
        assert_eq!(text_body(&response), "the name repo is taken");
        assert_eq!(log.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_posted_project_with_an_invalid_name_is_a_bad_request() {
        let log = Arc::new(Log::new());
        let addr = serve(log.clone()).await;

        let response = send(addr, "POST", "/api/projects", r#"{"name":"my app"}"#).await;

        assert!(response.starts_with("HTTP/1.1 400"));
        assert!(text_body(&response).starts_with("invalid name"));
        assert_eq!(log.0.lock().unwrap().len(), 1);
    }
}
