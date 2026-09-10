//! `percept review` - serves the embedded review page over HTTP on
//! `127.0.0.1`, on a port the OS picks, and opens it in the browser.
//! A presentation-layer peer of `cli` and `tui`: it has no chat logic
//! of its own. It serves the JSON the page reads (`GET /api/review`)
//! and writes (`POST /api/dispute`, `/api/confirm`, `/api/finish`)
//! over the same log and maps the CLI uses. Built on `axum`.
//!
//! The page is built into the binary at compile time - `build.rs`
//! copies `web/dist/index.html` into `OUT_DIR`, or writes a stub there
//! when the checkout has never run `npm run build` - so `cargo build`
//! never needs Node, and a checkout without the built page still
//! serves something explaining how to build it.

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;

use crate::core::{EventId, EventLog, HumanId, Schemas, Source};
use crate::server::review::Refused;

mod review;
#[cfg(test)]
mod tests;

/// The review page this binary was built with: the real one if
/// `web/dist/index.html` existed at build time, a stub explaining how
/// to build it otherwise.
const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

/// What every handler needs to fold or write the log: read fresh on
/// every `GET /api/review` call, never cached.
struct AppState {
    log: Arc<dyn EventLog>,
    schemas: Schemas,
    source: Source,
    me: Option<HumanId>,
}

/// `percept review` - binds a server on `127.0.0.1`, prints its URL,
/// opens it in the browser, and serves until the process is killed.
/// `log` and `schemas` are read fresh on every `GET /api/review`;
/// `source` says which project's events that cut reads; `me` is the
/// human every write the page makes is attributed to.
pub async fn run(
    log: Arc<dyn EventLog>,
    schemas: Schemas,
    source: Source,
    me: Option<HumanId>,
) -> Result<(), Box<dyn Error>> {
    let (listener, addr) = bind().await?;
    let url = format!("http://{addr}");
    println!("percept review at {url}");
    open_browser(&url);
    let state = Arc::new(AppState { log, schemas, source, me });
    serve(listener, state).await;
    Ok(())
}

/// Binds the server on an OS-picked port, without printing or opening
/// a browser - what a test binds against.
async fn bind() -> Result<(TcpListener, SocketAddr), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    Ok((listener, addr))
}

/// Serves requests on `listener` until the process is killed: `GET /`
/// and `GET /index.html` return the embedded page, `GET /api/review`
/// the queue `review::cut` folds fresh from the log, `POST /api/dispute`,
/// `POST /api/confirm`, and `POST /api/finish` each take one JSON body
/// and answer the appended event's id or a plain-text reason,
/// everything else 404s.
async fn serve(listener: TcpListener, state: Arc<AppState>) {
    let app = router(state);
    axum::serve(listener, app).await.expect("the review server never returns an error");
}

/// The route table every handler is registered on, shared by `serve`
/// and the tests that spawn it over a bound listener.
fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/api/review", get(api_review))
        .route("/api/dispute", post(api_dispute))
        .route("/api/confirm", post(api_confirm))
        .route("/api/finish", post(api_finish))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(PAGE)
}

async fn api_review(State(state): State<Arc<AppState>>) -> Response {
    let result = tokio::task::spawn_blocking(move || {
        review::cut(&*state.log, &state.schemas, &state.source).map_err(|err| err.to_string())
    })
    .await
    .expect("api_review's blocking fold never panics");
    match result {
        Ok(body) => Json(body).into_response(),
        Err(reason) => (StatusCode::INTERNAL_SERVER_ERROR, reason).into_response(),
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

async fn api_dispute(State(state): State<Arc<AppState>>, Json(body): Json<DisputeBody>) -> Response {
    write_response(tokio::task::spawn_blocking(move || {
        review::dispute(&*state.log, &state.schemas, &state.source, state.me, &body.map, &body.node, body.why)
    }))
    .await
}

async fn api_confirm(State(state): State<Arc<AppState>>, Json(body): Json<NodeBody>) -> Response {
    write_response(tokio::task::spawn_blocking(move || {
        review::confirm(&*state.log, &state.schemas, &state.source, state.me, &body.map, &body.node)
    }))
    .await
}

async fn api_finish(State(state): State<Arc<AppState>>, Json(body): Json<FinishBody>) -> Response {
    write_response(tokio::task::spawn_blocking(move || {
        review::finish(&*state.log, &state.schemas, &state.source, state.me, &body.map, &body.nodes)
    }))
    .await
}

/// Awaits a `spawn_blocking`'d write and turns its result into the
/// response every write route shares: the appended event's id as JSON,
/// or the refusal's reason as plain text with its status.
async fn write_response(
    task: tokio::task::JoinHandle<Result<EventId, Refused>>,
) -> Response {
    match task.await.expect("a write handler's blocking task never panics") {
        Ok(id) => Json(json!({ "event": id.as_uuid().to_string() })).into_response(),
        Err(Refused::Bad(reason)) => (StatusCode::BAD_REQUEST, reason).into_response(),
        Err(Refused::NotFound(reason)) => (StatusCode::NOT_FOUND, reason).into_response(),
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
