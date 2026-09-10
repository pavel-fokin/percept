use std::io::{Read, Write};
use std::net::TcpStream;

use super::*;

/// Binds a server on a spare port, serves it on a background thread,
/// and returns its address for a test to connect to.
fn spawn() -> std::net::SocketAddr {
    let server = bind().expect("bind a server on a spare port");
    let addr = server.server_addr().to_ip().expect("server bound to an IP address");
    std::thread::spawn(move || serve(server));
    addr
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
fn unknown_path_returns_404() {
    let addr = spawn();
    let response = get(addr, "/nope");
    assert!(response.starts_with("HTTP/1.0 404"), "{response}");
}
