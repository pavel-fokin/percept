use std::io::{Read, Write};
use std::net::TcpStream;

use serde_json::json;

use super::*;
use crate::core::testing::{human, node_added_by, node_id, schemas, source, FakeLog};
use crate::core::{Actor, Payload};

/// Binds a server on a spare port, serves it on a spawned task over an
/// empty in-memory log and the built-in schemas, and returns its
/// address for a test to connect to.
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
        schemas: schemas(),
        source: source("test"),
        me: human(),
        since: None,
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

/// Sends a raw HTTP/1.1 POST of `body` for `path` to `addr` and returns
/// the status line and the response text past the headers.
async fn post(addr: std::net::SocketAddr, path: &str, body: &serde_json::Value) -> (String, String) {
    let text = body.to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{text}",
        text.len()
    );
    let response = send(addr, request).await;
    let (status, body) = split(&response);
    (status.to_string(), body.to_string())
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
    assert!(response.contains("<title>percept review</title>"), "{response}");
}

#[tokio::test]
async fn root_with_a_query_string_still_returns_the_embedded_page() {
    let addr = spawn().await;
    let response = get(addr, "/?x=1").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("<title>percept review</title>"), "{response}");
}

#[tokio::test]
async fn api_review_returns_json_with_a_maps_array() {
    let addr = spawn().await;
    let response = get(addr, "/api/review").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("application/json"), "{response}");
    let (_, body) = split(&response);
    let json: serde_json::Value = serde_json::from_str(body).expect("valid JSON");
    assert!(json["maps"].is_array(), "{json}");
}

#[tokio::test]
async fn unknown_path_returns_404() {
    let addr = spawn().await;
    let response = get(addr, "/nope").await;
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
}

#[tokio::test]
async fn change_posted_with_a_why_appends_a_node_changed_naming_the_node_and_the_why() {
    let node = node_added_by(Actor::Agent, "decision", "ship it");
    let (log, addr) = spawn_over(vec![node.clone()]).await;

    let (status, body) =
        post(addr, "/api/change", &json!({ "map": "decisions", "node": "d1", "why": "not yet" })).await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status} {body}");

    let events = log.load().unwrap();
    let changed = events
        .iter()
        .find_map(|event| match event.payload() {
            Payload::NodeChanged { node: changed, why, .. } => Some((*changed, why.clone())),
            _ => None,
        })
        .expect("a node.changed event");
    assert_eq!(changed, (node_id(&node), Some("not yet".to_string())));
}

#[tokio::test]
async fn change_with_a_blank_why_is_refused_with_400_and_appends_nothing() {
    let node = node_added_by(Actor::Agent, "decision", "ship it");
    let (log, addr) = spawn_over(vec![node]).await;

    let (status, body) =
        post(addr, "/api/change", &json!({ "map": "decisions", "node": "d1", "why": "   " })).await;
    assert!(status.starts_with("HTTP/1.1 400"), "{status} {body}");

    assert_eq!(log.load().unwrap().len(), 1, "nothing beyond the seeded node.added");
}

#[tokio::test]
async fn an_unknown_node_id_is_404() {
    let addr = spawn().await;
    let (status, body) =
        post(addr, "/api/change", &json!({ "map": "decisions", "node": "d99", "why": "not yet" })).await;
    assert!(status.starts_with("HTTP/1.1 404"), "{status} {body}");
}

#[tokio::test]
async fn a_change_takes_the_node_out_of_the_humans_review_queue() {
    let node = node_added_by(Actor::Agent, "decision", "ship it");
    let (_, addr) = spawn_over(vec![node]).await;

    let (status, _) =
        post(addr, "/api/change", &json!({ "map": "decisions", "node": "d1", "why": "not yet" })).await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");

    let response = get(addr, "/api/review").await;
    let (_, body) = split(&response);
    let json: serde_json::Value = serde_json::from_str(body).expect("valid JSON");
    let decisions = json["maps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|map| map["name"] == "decisions")
        .expect("a decisions map");
    // The human's own write is not theirs to review: the node's last
    // change is the human's, so it leaves the queue.
    assert!(decisions["groups"].as_array().unwrap().is_empty(), "{decisions}");
}
