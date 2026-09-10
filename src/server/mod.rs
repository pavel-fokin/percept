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

use tiny_http::{Header, Method, Response, Server};

use crate::core::{EventLog, Schemas, Source};

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
/// `source` says which project's events that cut reads.
pub fn run(log: Arc<dyn EventLog>, schemas: Arc<Schemas>, source: Source) -> Result<(), Box<dyn Error>> {
    let server = bind()?;
    let url = format!("http://{}", server.server_addr());
    println!("percept review at {url}");
    if is_stub() {
        eprintln!("the review page is not built; see the page for how");
    }
    open_browser(&url);
    serve(server, log, schemas, source);
    Ok(())
}

/// Binds the server on an OS-picked port, without printing or opening
/// a browser - what a test binds against.
fn bind() -> Result<Server, Box<dyn Error>> {
    Server::http("127.0.0.1:0").map_err(|err| err.to_string().into())
}

/// Serves requests on `server` until the process is killed: `GET /`
/// and `GET /index.html` return the embedded page, `GET /api/review`
/// the queue `review::cut` folds fresh from `log`, everything else
/// 404s.
fn serve(server: Server, log: Arc<dyn EventLog>, schemas: Arc<Schemas>, source: Source) {
    for request in server.incoming_requests() {
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
