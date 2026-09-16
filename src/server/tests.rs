use std::io::{Read, Write};
use std::net::TcpStream;

use super::*;
use crate::core::testing::{node_added_by, source, Fixture, FakeLog};
use crate::core::{Actor, Payload};

/// Binds a server on a spare port, serves it on a spawned task over an
/// empty in-memory log, and returns its address for a test to connect
/// to.
async fn spawn() -> std::net::SocketAddr {
    spawn_over(Vec::new()).await.1
}

/// `spawn`, seeded with `events` and handing the test back the same
/// `FakeLog` the server writes to, so it can read a write's effect back
/// with `.load()`. The server runs on a spawned task the same way
/// `main` runs it, and outlives the test - the process exits when the
/// test binary does.
async fn spawn_over(events: Vec<crate::core::Event>) -> (std::sync::Arc<FakeLog>, std::net::SocketAddr) {
    let (listener, addr) = bind().await.expect("bind a server on a spare port");
    let log = std::sync::Arc::new(FakeLog::seeded(events));
    let handed_back = log.clone();
    let state = std::sync::Arc::new(AppState {
        log: log.clone() as std::sync::Arc<dyn crate::core::EventLog>,
        source: source("test"),
        opened: crate::shared::Timestamp::now(),
    });
    tokio::spawn(serve(listener, state));
    (handed_back, addr)
}

/// Sends a raw HTTP/1.1 request to `addr` and returns the full response
/// text - what `get` and `post` both parse. Every request carries
/// `Connection: close`, so the server closes the socket once it has
/// answered and `read_to_string` sees EOF; shutting down the write half
/// ourselves first would race the server's own read of the request.
/// Runs the blocking socket I/O on a blocking thread, since the server
/// itself runs on the same runtime.
async fn send(addr: std::net::SocketAddr, request: String) -> String {
    tokio::task::spawn_blocking(move || {
        let mut stream = TcpStream::connect(addr).expect("connect to the running server");
        stream.write_all(request.as_bytes()).expect("write the request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read the response");
        response
    })
    .await
    .expect("the blocking request never panics")
}

/// `response`'s status line and its body past the headers.
fn split(response: &str) -> (&str, &str) {
    let status = response.lines().next().unwrap_or_default();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
    (status, body)
}

/// Sends a raw HTTP/1.1 GET for `path` to `addr` and returns the
/// response text.
async fn get(addr: std::net::SocketAddr, path: &str) -> String {
    send(addr, format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")).await
}

#[tokio::test]
async fn root_returns_the_embedded_page() {
    let addr = spawn().await;
    let response = get(addr, "/").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("<title>percept</title>"), "{response}");
}

#[tokio::test]
async fn root_with_a_query_string_still_returns_the_embedded_page() {
    let addr = spawn().await;
    let response = get(addr, "/?x=1").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("<title>percept</title>"), "{response}");
}

#[tokio::test]
async fn a_path_the_page_routes_itself_is_served_the_page() {
    let addr = spawn().await;
    let response = get(addr, "/log?q=drift").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("<title>percept</title>"), "{response}");
}

#[tokio::test]
async fn an_unknown_endpoint_under_api_is_not_served_the_page() {
    let addr = spawn().await;
    let response = get(addr, "/api/nope").await;
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    assert!(!response.contains("<title>percept</title>"), "{response}");
}

/// A `message.received` from `/test`, for a query that has to tell one
/// kind of event from another.
fn message(content: &str) -> crate::core::Event {
    crate::core::Event::new(
        Actor::Agent,
        source("test"),
        None,
        Payload::MessageReceived {
            content: content.to_string(),
        },
    )
}

/// `get`, split into an owned status line and body - what a route test
/// reads, since `split` borrows the response it was handed.
async fn get_parts(addr: std::net::SocketAddr, path: &str) -> (String, String) {
    let response = get(addr, path).await;
    let (status, body) = split(&response);
    (status.to_string(), body.to_string())
}

/// `get_parts`, with the body parsed - every route here answers JSON.
async fn get_json(addr: std::net::SocketAddr, path: &str) -> (String, serde_json::Value) {
    let (status, body) = get_parts(addr, path).await;
    (status, serde_json::from_str(&body).expect("a JSON body"))
}

#[tokio::test]
async fn api_events_serves_only_this_projects_events() {
    let events = vec![
        message("in this project"),
        crate::core::testing::node_added_at("/elsewhere", "claim", "in another"),
    ];
    let (_log, addr) = spawn_over(events).await;
    let (status, body) = get_json(addr, "/api/events").await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    assert_eq!(body["events"].as_array().unwrap().len(), 1);
    assert_eq!(body["total"], 1);
}

#[tokio::test]
async fn api_events_reports_the_total_from_before_size_cut_it() {
    let events = vec![message("one"), message("two"), message("three")];
    let (_log, addr) = spawn_over(events).await;
    let (_, body) = get_json(addr, "/api/events?size=2").await;
    assert_eq!(body["events"].as_array().unwrap().len(), 2);
    assert_eq!(body["total"], 3);
}

#[tokio::test]
async fn api_events_filters_by_the_type_parameter() {
    let events = vec![message("said"), node_added_by(Actor::Agent, "claim", "recorded")];
    let (_log, addr) = spawn_over(events).await;
    let (status, body) = get_json(addr, "/api/events?type=message.received").await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "message.received");
}

#[tokio::test]
async fn api_events_filters_by_the_actor_parameter() {
    let human_message = crate::core::Event::new(
        Actor::Human(crate::core::testing::human()),
        source("test"),
        None,
        Payload::MessageReceived {
            content: "said".to_string(),
        },
    );
    let events = vec![human_message, node_added_by(Actor::Agent, "claim", "recorded")];
    let (_log, addr) = spawn_over(events).await;
    let (status, body) = get_json(addr, "/api/events?actor=human").await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["actor"]["kind"], "human");
}

#[tokio::test]
async fn api_events_refuses_a_blank_contains_and_says_why() {
    let addr = spawn().await;
    let (status, body) = get_parts(addr, "/api/events?contains=").await;
    assert!(status.starts_with("HTTP/1.1 400"), "{status}");
    assert!(body.contains("contains"), "{body}");
}

#[tokio::test]
async fn api_event_returns_the_whole_event_by_id() {
    let event = message("the one we ask for");
    let id = event.id().as_uuid().to_string();
    let (_log, addr) = spawn_over(vec![event]).await;
    let (status, body) = get_json(addr, &format!("/api/events/{id}")).await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    assert_eq!(body["event"]["payload"]["content"], "the one we ask for");
}

#[tokio::test]
async fn api_event_is_not_found_for_an_event_in_another_project() {
    let event = crate::core::testing::node_added_at("/elsewhere", "claim", "in another");
    let id = event.id().as_uuid().to_string();
    let (_log, addr) = spawn_over(vec![event]).await;
    let (status, _) = get_parts(addr, &format!("/api/events/{id}")).await;
    assert!(status.starts_with("HTTP/1.1 404"), "{status}");
}

/// `/api/maps/{name}` answers for the `root` the query names, not
/// `state.source.path` - proving the path segment binds `name` and the
/// query string binds `root`, `around`, and `depth` together.
#[tokio::test]
async fn api_maps_cuts_the_named_project_root_around_a_node() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/decisions.toml",
        "name = \"decisions\"\npurpose = \"test\"\nheadlines = [\"concept\"]\n\n\
         [[node]]\nkind = \"concept\"\n\n[[node]]\nkind = \"question\"\n\n\
         [[edge]]\nkind = \"about\"\nfrom = \"question\"\nto = \"concept\"\n",
    );
    let concept = crate::core::Event::new(
        Actor::Agent,
        crate::core::Source {
            name: "test".to_string(),
            path: fixture.path().to_path_buf(),
        },
        None,
        Payload::NodeAdded {
            map: "decisions".to_string(),
            node: crate::core::NodeId::new(),
            kind: "concept".to_string(),
            name: "the rule".to_string(),
            properties: Default::default(),
            sources: Vec::new(),
            seq: 0,
        },
    );
    let (_log, addr) = spawn_over(vec![concept]).await;

    let path = format!("/api/maps/decisions?root={}", fixture.path().to_string_lossy());
    let (status, body) = get_json(addr, &path).await;

    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    assert_eq!(body["map"]["name"], "decisions");
    assert_eq!(body["nodes"].as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body["nodes"][0]["kind"], "concept");
}

/// Unlike `/api/events`, `/api/projects` is not scoped to the server's
/// own source - a project this server never opened still shows up.
#[tokio::test]
async fn api_projects_serves_every_project_the_log_holds_not_only_this_ones() {
    let events = vec![
        message("in this project"),
        crate::core::testing::node_added_at("/elsewhere", "claim", "in another"),
    ];
    let (_log, addr) = spawn_over(events).await;
    let (status, body) = get_json(addr, "/api/projects").await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    let projects = body["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2, "{body}");
    let paths: Vec<&str> = projects.iter().map(|project| project["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"/elsewhere"), "{body}");
    assert!(paths.contains(&crate::core::testing::ROOT), "{body}");
    for project in projects {
        assert!(project["name"].is_string(), "{body}");
        assert!(project["events"].is_number(), "{body}");
        assert!(project["last_active"].is_string(), "{body}");
        assert!(project["maps"].is_array(), "{body}");
    }
}
