//! `percept review` - serves the embedded review page over HTTP on
//! `127.0.0.1`, on a port the OS picks, and opens it in the browser.
//! A presentation-layer peer of `cli` and `tui`: it has no chat logic
//! of its own. It serves the JSON the page reads (`GET /api/review`)
//! and writes (`POST /api/dispute`, `/api/confirm`, `/api/finish`)
//! over the same log and maps the CLI uses.
//!
//! The page is built into the binary at compile time - `build.rs`
//! copies `web/dist/index.html` into `OUT_DIR`, or writes a stub there
//! when the checkout has never run `npm run build` - so `cargo build`
//! never needs Node, and a checkout without the built page still
//! serves something explaining how to build it.

use std::error::Error;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, ResponseBox, Server};

use crate::core::{EventId, EventLog, HumanId, Schemas, Source};
use crate::server::review::Refused;

mod review;
#[cfg(test)]
mod tests;

/// The review page this binary was built with: the real one if
/// `web/dist/index.html` existed at build time, a stub explaining how
/// to build it otherwise.
const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

/// `percept review` - binds a server on `127.0.0.1`, prints its URL,
/// opens it in the browser, and serves until the process is killed.
/// `log` and `schemas` are read fresh on every `GET /api/review`;
/// `source` says which project's events that cut reads; `me` is the
/// human every write the page makes is attributed to.
pub fn run(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: Source,
    me: Option<HumanId>,
) -> Result<(), Box<dyn Error>> {
    let server = bind()?;
    let url = format!("http://{}", server.server_addr());
    println!("percept review at {url}");
    open_browser(&url);
    serve(server, log, schemas, source, me);
    Ok(())
}

/// Binds the server on an OS-picked port, without printing or opening
/// a browser - what a test binds against.
fn bind() -> Result<Server, Box<dyn Error>> {
    Server::http("127.0.0.1:0").map_err(|err| err.to_string().into())
}

/// A response with `Content-Type: content_type` and `status`, whatever
/// the caller's `body`.
fn respond(body: impl Into<Vec<u8>>, content_type: &str, status: u16) -> ResponseBox {
    let header = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes())
        .expect("content-type header value is valid ASCII");
    Response::from_data(body.into())
        .with_status_code(status)
        .with_header(header)
        .boxed()
}

/// Reads `request`'s body in full, parses it as `B`, and hands it to
/// `act` - the one write path `/api/dispute`, `/api/confirm`, and
/// `/api/finish` share: a body that fails to parse, or an `act` that
/// refuses it, both answer with the reason as plain text.
fn write<B: DeserializeOwned>(
    request: &mut Request,
    act: impl FnOnce(B) -> Result<EventId, Refused>,
) -> ResponseBox {
    let body = match read_body::<B>(request) {
        Ok(body) => body,
        Err(reason) => return respond(reason, "text/plain; charset=utf-8", 400),
    };
    match act(body) {
        Ok(id) => respond(
            json!({ "event": id.as_uuid().to_string() }).to_string(),
            "application/json; charset=utf-8",
            200,
        ),
        Err(Refused::Bad(reason)) => respond(reason, "text/plain; charset=utf-8", 400),
        Err(Refused::NotFound(reason)) => respond(reason, "text/plain; charset=utf-8", 404),
    }
}

/// Serves requests on `server` until the process is killed: `GET /`
/// and `GET /index.html` return the embedded page, `GET /api/review`
/// the queue `review::cut` folds fresh from `log`, `POST /api/dispute`,
/// `POST /api/confirm`, and `POST /api/finish` each take one JSON body
/// and answer the appended event's id or a plain-text reason,
/// everything else 404s.
fn serve(server: Server, log: &dyn EventLog, schemas: &Schemas, source: Source, me: Option<HumanId>) {
    for mut request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("").to_string();
        let response = match (request.method(), path.as_str()) {
            (Method::Get, "/" | "/index.html") => {
                respond(PAGE, "text/html; charset=utf-8", 200)
            }
            (Method::Get, "/api/review") => match review::cut(log, schemas, &source) {
                Ok(body) => respond(
                    serde_json::to_string(&body).expect("ReviewResponse always serializes"),
                    "application/json; charset=utf-8",
                    200,
                ),
                Err(err) => respond(err.to_string(), "text/plain; charset=utf-8", 500),
            },
            (Method::Post, "/api/dispute") => write(&mut request, |body: DisputeBody| {
                review::dispute(log, schemas, &source, me, &body.map, &body.node, body.why)
            }),
            (Method::Post, "/api/confirm") => write(&mut request, |body: NodeBody| {
                review::confirm(log, schemas, &source, me, &body.map, &body.node)
            }),
            (Method::Post, "/api/finish") => write(&mut request, |body: FinishBody| {
                review::finish(log, schemas, &source, me, &body.map, &body.nodes)
            }),
            _ => Response::empty(404).boxed(),
        };
        let _ = request.respond(response);
    }
}

/// `{"map": ..., "node": ...}`, the body `/api/confirm` takes.
#[derive(Deserialize)]
struct NodeBody {
    map: String,
    node: String,
}

/// The body `/api/dispute` takes.
#[derive(Deserialize)]
struct DisputeBody {
    map: String,
    node: String,
    why: String,
}

/// The body `/api/finish` takes.
#[derive(Deserialize)]
struct FinishBody {
    map: String,
    nodes: Vec<String>,
}

/// Reads `request`'s body in full and parses it as `T` - a body that
/// isn't valid JSON, or doesn't match the shape a route expects, reads
/// as a refused write the same as any other.
fn read_body<T: DeserializeOwned>(request: &mut Request) -> Result<T, String> {
    let mut text = String::new();
    request
        .as_reader()
        .read_to_string(&mut text)
        .map_err(|err| err.to_string())?;
    serde_json::from_str(&text).map_err(|err| err.to_string())
}

/// Opens `url` in the user's default browser: `open` on macOS,
/// `xdg-open` elsewhere. Failure to open is ignored - the URL is
/// already printed either way.
fn open_browser(url: &str) {
    let command = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(command).arg(url).spawn();
}
