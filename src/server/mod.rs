//! `percept review` - serves the embedded review page over HTTP on
//! `127.0.0.1`, on a port the OS picks, and opens it in the browser.
//! A presentation-layer peer of `cli` and `tui`: it has no chat logic
//! of its own, and later serves the JSON the page reads and writes
//! over the same log and maps the CLI uses.
//!
//! The page is built into the binary at compile time - `build.rs`
//! copies `web/dist/index.html` into `OUT_DIR`, or writes a stub there
//! when the checkout has never run `npm run build` - so `cargo build`
//! never needs Node, and a checkout without the built page still
//! serves something explaining how to build it.

use std::error::Error;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;
use tiny_http::{Header, Method, Response, Server};

use crate::core::{EventLog, HumanId, Schemas, Source};
use crate::server::review::ApiOutcome;

mod review;
#[cfg(test)]
mod tests;

/// The review page this binary was built with: the real one if
/// `web/dist/index.html` existed at build time, a stub explaining how
/// to build it otherwise. `build.rs` sets `PERCEPT_REVIEW_PAGE` to say
/// which.
const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

/// Whether `PAGE` is the real review page or the stub `build.rs`
/// writes when `web/dist/index.html` doesn't exist.
fn is_stub() -> bool {
    env!("PERCEPT_REVIEW_PAGE") == "stub"
}

/// `percept review` - binds a server on `127.0.0.1`, prints its URL,
/// opens it in the browser, and serves until the process is killed.
/// `log` and `schemas` are read fresh on every `GET /api/review`;
/// `source` says which project's events that cut reads; `me` is the
/// human every write the page makes is attributed to.
pub fn run(
    log: Arc<dyn EventLog>,
    schemas: Arc<Schemas>,
    source: Source,
    me: Option<HumanId>,
) -> Result<(), Box<dyn Error>> {
    let server = bind()?;
    let url = format!("http://{}", server.server_addr());
    println!("percept review at {url}");
    if is_stub() {
        eprintln!("the review page is not built; see the page for how");
    }
    open_browser(&url);
    serve(server, log, schemas, source, me);
    Ok(())
}

/// Binds the server on an OS-picked port, without printing or opening
/// a browser - what a test binds against.
fn bind() -> Result<Server, Box<dyn Error>> {
    Server::http("127.0.0.1:0").map_err(|err| err.to_string().into())
}

/// Serves requests on `server` until the process is killed: `GET /`
/// and `GET /index.html` return the embedded page, `GET /api/review`
/// the queue `review::cut` folds fresh from `log`, `POST /api/dispute`,
/// `POST /api/confirm`, and `POST /api/finish` each take one JSON body
/// and answer the appended event's id or a plain-text reason,
/// everything else 404s.
fn serve(server: Server, log: Arc<dyn EventLog>, schemas: Arc<Schemas>, source: Source, me: Option<HumanId>) {
    for mut request in server.incoming_requests() {
        let response = match (request.method(), request.url()) {
            (Method::Get, "/" | "/index.html") => {
                let header = Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"text/html; charset=utf-8"[..],
                )
                .expect("static header name and value are valid ASCII");
                Response::from_string(PAGE).with_header(header).boxed()
            }
            (Method::Get, "/api/review") => review_response(log.as_ref(), &schemas, &source),
            (Method::Post, "/api/dispute") => {
                dispute_response(&mut request, log.as_ref(), &schemas, &source, me)
            }
            (Method::Post, "/api/confirm") => {
                confirm_response(&mut request, log.as_ref(), &schemas, &source, me)
            }
            (Method::Post, "/api/finish") => {
                finish_response(&mut request, log.as_ref(), &schemas, &source, me)
            }
            _ => Response::empty(404).boxed(),
        };
        let _ = request.respond(response);
    }
}

/// `GET /api/review`'s response: `review::cut`'s JSON, or its error as
/// a 500 - the log or a schema failing to fold is the one way this can
/// go wrong, and a reader gets the message rather than a dropped
/// connection.
fn review_response(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
) -> tiny_http::ResponseBox {
    match review::cut(log, schemas, source) {
        Ok(body) => {
            let header = Header::from_bytes(
                &b"Content-Type"[..],
                &b"application/json; charset=utf-8"[..],
            )
            .expect("static header name and value are valid ASCII");
            Response::from_string(body.to_string()).with_header(header).boxed()
        }
        Err(err) => Response::from_string(err.to_string()).with_status_code(500).boxed(),
    }
}

/// `{"map": ..., "node": ...}`, the body `/api/confirm` takes and
/// `/api/dispute` extends with `why`.
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
/// as `Bad` the same as any other refused write.
fn read_body<T: serde::de::DeserializeOwned>(request: &mut tiny_http::Request) -> Result<T, String> {
    let mut text = String::new();
    request
        .as_reader()
        .read_to_string(&mut text)
        .map_err(|err| err.to_string())?;
    serde_json::from_str(&text).map_err(|err| err.to_string())
}

/// `ApiOutcome` as tiny_http's response: the event's id as `{"event":
/// ...}` on 200, or the reason as plain text on 400 or 404.
fn outcome_response(outcome: ApiOutcome) -> tiny_http::ResponseBox {
    match outcome {
        ApiOutcome::Ok(id) => {
            let header = Header::from_bytes(
                &b"Content-Type"[..],
                &b"application/json; charset=utf-8"[..],
            )
            .expect("static header name and value are valid ASCII");
            Response::from_string(json!({ "event": id.as_uuid().to_string() }).to_string())
                .with_header(header)
                .boxed()
        }
        ApiOutcome::Bad(reason) => text_response(reason, 400),
        ApiOutcome::NotFound(reason) => text_response(reason, 404),
    }
}

/// A plain-text response with `status` - what a rejected body, and
/// `ApiOutcome`'s error cases, both answer with.
fn text_response(text: String, status: u16) -> tiny_http::ResponseBox {
    let header = Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..])
        .expect("static header name and value are valid ASCII");
    Response::from_string(text)
        .with_status_code(status)
        .with_header(header)
        .boxed()
}

/// `POST /api/dispute`'s response: `review::dispute`'s outcome, or 400
/// naming a body that failed to parse.
fn dispute_response(
    request: &mut tiny_http::Request,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
) -> tiny_http::ResponseBox {
    match read_body::<DisputeBody>(request) {
        Ok(body) => outcome_response(review::dispute(
            log, schemas, source, me, &body.map, &body.node, body.why,
        )),
        Err(reason) => text_response(reason, 400),
    }
}

/// `POST /api/confirm`'s response: `review::confirm`'s outcome, or 400
/// naming a body that failed to parse.
fn confirm_response(
    request: &mut tiny_http::Request,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
) -> tiny_http::ResponseBox {
    match read_body::<NodeBody>(request) {
        Ok(body) => outcome_response(review::confirm(log, schemas, source, me, &body.map, &body.node)),
        Err(reason) => text_response(reason, 400),
    }
}

/// `POST /api/finish`'s response: `review::finish`'s outcome, or 400
/// naming a body that failed to parse.
fn finish_response(
    request: &mut tiny_http::Request,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
) -> tiny_http::ResponseBox {
    match read_body::<FinishBody>(request) {
        Ok(body) => outcome_response(review::finish(log, schemas, source, me, &body.map, &body.nodes)),
        Err(reason) => text_response(reason, 400),
    }
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
