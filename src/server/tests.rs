use std::io::{Read, Write};
use std::net::TcpStream;

use serde_json::json;

use super::*;
use crate::core::testing::{human, node_added, node_added_by, node_id, schemas, source, FakeLog};
use crate::core::{Actor, Payload};

/// Binds a server on a spare port, serves it on a background thread
/// over an empty in-memory log and the built-in schemas, and returns
/// its address for a test to connect to.
fn spawn() -> std::net::SocketAddr {
    spawn_over(Vec::new()).1
}

/// `spawn`, seeded with `events` and handing the test back the same
/// `FakeLog` the server writes to, so it can read a write's effect back
/// with `.load()`.
fn spawn_over(events: Vec<crate::core::Event>) -> (Arc<FakeLog>, std::net::SocketAddr) {
    let server = bind().expect("bind a server on a spare port");
    let addr = server.server_addr().to_ip().expect("server bound to an IP address");
    let log = Arc::new(FakeLog::seeded(events));
    let schemas = Arc::new(schemas());
    let src = source("test");
    let served: Arc<dyn EventLog> = log.clone();
    std::thread::spawn(move || serve(server, served, schemas, src, human()));
    (log, addr)
}

/// Sends a raw HTTP/1.0 POST of `body` for `path` to `addr` and returns
/// the status line and the response text past the headers.
fn post(addr: std::net::SocketAddr, path: &str, body: &serde_json::Value) -> (String, String) {
    let text = body.to_string();
    let mut stream = TcpStream::connect(addr).expect("connect to the running server");
    let request = format!(
        "POST {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{text}",
        text.len()
    );
    stream
        .write_all(request.as_bytes())
        .expect("write the request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read the response");
    let status = response.lines().next().unwrap_or_default().to_string();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or_default().to_string();
    (status, body)
}

/// Sends a raw HTTP/1.0 GET for `path` to `addr` and returns the
/// response text.
fn get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect to the running server");
    let request = format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .expect("write the request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read the response");
    response
}

#[test]
fn root_returns_the_embedded_page() {
    let addr = spawn();
    let response = get(addr, "/");
    assert!(response.starts_with("HTTP/1.0 200"), "{response}");
    assert!(response.contains("<title>percept review</title>"), "{response}");
}

#[test]
fn root_with_a_query_string_still_returns_the_embedded_page() {
    let addr = spawn();
    let response = get(addr, "/?x=1");
    assert!(response.starts_with("HTTP/1.0 200"), "{response}");
    assert!(response.contains("<title>percept review</title>"), "{response}");
}

#[test]
fn api_review_returns_json_with_a_maps_array() {
    let addr = spawn();
    let response = get(addr, "/api/review");
    assert!(response.starts_with("HTTP/1.0 200"), "{response}");
    assert!(response.contains("application/json"), "{response}");
    let body = response.split("\r\n\r\n").nth(1).expect("a body past the headers");
    let json: serde_json::Value = serde_json::from_str(body).expect("valid JSON");
    assert!(json["maps"].is_array(), "{json}");
}

#[test]
fn unknown_path_returns_404() {
    let addr = spawn();
    let response = get(addr, "/nope");
    assert!(response.starts_with("HTTP/1.0 404"), "{response}");
}

#[test]
fn dispute_posted_with_a_why_appends_a_claim_disputed_naming_the_node_and_the_why() {
    let node = node_added_by(Actor::Agent, "decision", "ship it");
    let (log, addr) = spawn_over(vec![node.clone()]);

    let (status, body) = post(addr, "/api/dispute", &json!({ "map": "decisions", "node": "d1", "why": "not yet" }));
    assert!(status.starts_with("HTTP/1.0 200"), "{status} {body}");

    let events = log.load().unwrap();
    let disputed = events
        .iter()
        .find_map(|event| match event.payload() {
            Payload::ClaimDisputed { node: disputed, why, .. } => Some((*disputed, why.clone())),
            _ => None,
        })
        .expect("a claim.disputed event");
    assert_eq!(disputed, (node_id(&node), "not yet".to_string()));
}

#[test]
fn dispute_with_a_blank_why_is_refused_with_400_and_appends_nothing() {
    let node = node_added_by(Actor::Agent, "decision", "ship it");
    let (log, addr) = spawn_over(vec![node]);

    let (status, body) = post(addr, "/api/dispute", &json!({ "map": "decisions", "node": "d1", "why": "   " }));
    assert!(status.starts_with("HTTP/1.0 400"), "{status} {body}");

    assert_eq!(log.load().unwrap().len(), 1, "nothing beyond the seeded node.added");
}

#[test]
fn confirm_appends_a_claim_confirmed() {
    let node = node_added_by(Actor::Agent, "decision", "ship it");
    let (log, addr) = spawn_over(vec![node.clone()]);

    let (status, body) = post(addr, "/api/confirm", &json!({ "map": "decisions", "node": "d1" }));
    assert!(status.starts_with("HTTP/1.0 200"), "{status} {body}");

    let events = log.load().unwrap();
    let confirmed = events.iter().any(|event| {
        matches!(event.payload(), Payload::ClaimConfirmed { node: confirmed, .. } if *confirmed == node_id(&node))
    });
    assert!(confirmed, "expected a claim.confirmed for the disputed node");
}

#[test]
fn finish_appends_one_review_finished_naming_the_resolved_nodes_and_skipping_a_user_written_one() {
    let decision = node_added_by(Actor::Agent, "decision", "ship it");
    let question = node_added("question", "should we ship it");
    let (log, addr) = spawn_over(vec![decision.clone(), question.clone()]);

    let (status, body) = post(addr, "/api/finish", &json!({ "map": "decisions", "nodes": ["d1", "q1"] }));
    assert!(status.starts_with("HTTP/1.0 200"), "{status} {body}");

    let events = log.load().unwrap();
    let finished = events
        .iter()
        .find_map(|event| match event.payload() {
            Payload::ReviewFinished { nodes, .. } => Some(nodes.clone()),
            _ => None,
        })
        .expect("a review.finished event");
    assert_eq!(finished, vec![node_id(&decision)], "the human-written question is skipped");
}

#[test]
fn an_unknown_node_id_is_404() {
    let addr = spawn();
    let (status, body) = post(addr, "/api/confirm", &json!({ "map": "decisions", "node": "d99" }));
    assert!(status.starts_with("HTTP/1.0 404"), "{status} {body}");
}
