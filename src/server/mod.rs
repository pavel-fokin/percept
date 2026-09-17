//! `percept web` - serves the embedded page over HTTP on
//! `127.0.0.1`, on a port the OS picks, and opens it in the browser.
//! A presentation-layer peer of `cli` and `tui`: it has no chat logic
//! of its own. It serves the JSON the page reads (`GET /api/events`,
//! `GET /api/events/{id}`) over the same log the CLI uses, and
//! `GET /api/projects`, every project the log holds - not scoped to
//! this server's own, unlike the other two. Built on `axum`.
//!
//! The page is built into the binary at compile time - `build.rs`
//! copies `web/dist/index.html` into `OUT_DIR`, or writes a stub there
//! when the checkout has never run `npm run build` - so `cargo build`
//! never needs Node, and a checkout without the built page still
//! serves something explaining how to build it.
//!
//! `GET /api/maps/{name}` is a third route unscoped to this server's
//! own project, like `GET /api/projects`: `root` names the project
//! whose own schemas and events the map is folded from.

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use tokio::net::TcpListener;

use crate::core::{Event, EventLog, Source};
use crate::server::events::Error as EventsError;
use crate::shared::Timestamp;

mod events;
mod maps;
mod projects;
#[cfg(test)]
mod tests;

/// The page this binary was built with: the real one if
/// `web/dist/index.html` existed at build time, a stub explaining how
/// to build it otherwise.
const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

/// The writer name every event the server appends carries, so a
/// session it opens is never confused with the CLI's own.
const SOURCE_NAME: &str = "percept-web";

/// What every handler needs to read the log: read fresh on every
/// request, never cached. `opened` is when this view started - the
/// moment a "since last session" count is measured back from, held
/// still for as long as the view is open. Opening records a
/// `session.started` of its own, so a count read live would always
/// measure against this server and come back zero.
struct AppState {
    log: Arc<dyn EventLog>,
    source: Source,
    opened: Timestamp,
}

/// `percept web` - binds a server on `127.0.0.1`, prints its URL,
/// opens it in the browser, and serves until the process is killed.
/// `log` is read fresh on every request; `source`'s path says which
/// project's events the page reads, its own name replaced by
/// `percept-web`. Appends one `session.started` for that source before
/// serving - that a surface opened is a real entry in the log.
pub async fn run(log: Arc<dyn EventLog>, source: Source) -> Result<(), Box<dyn Error>> {
    let source = Source {
        name: SOURCE_NAME.to_string(),
        path: source.path,
    };
    let (listener, addr) = bind().await?;
    // The event lands only once the page can be served, so a bind that
    // fails records nothing.
    let opened = {
        let log = Arc::clone(&log);
        let marker = Event::session_started(source.clone());
        let opened = marker.created_at();
        tokio::task::spawn_blocking(move || log.append(&marker).map_err(|err| err.to_string()))
            .await
            .expect("appending session.started never panics")?;
        opened
    };
    let url = format!("http://{addr}");
    println!("percept web at {url}");
    open_browser(&url);
    let state = Arc::new(AppState { log, source, opened });
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

/// Serves requests on `listener` until the process is killed: `GET
/// /api/events` and `GET /api/events/{id}` return the project's own
/// log, `GET /api/projects` every project the log holds, `GET
/// /api/maps/{name}` one project's map cut to a reader's ask, and
/// every other path the page, which routes itself. An unknown path
/// under `/api` is a 404, never the page: a request for data that
/// answers with HTML is harder to read than one that says it found
/// nothing.
async fn serve(listener: TcpListener, state: Arc<AppState>) {
    let app = router(state);
    axum::serve(listener, app).await.expect("the web server never returns an error");
}

/// The route table every handler is registered on, shared by `serve`
/// and the tests that spawn it over a bound listener.
fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/events", get(api_events))
        .route("/api/events/{id}", get(api_event))
        .route("/api/projects", get(api_projects))
        .route("/api/maps/{name}", get(api_maps))
        .fallback(page)
        .with_state(state)
}

/// Every path the API does not own. The page carries its own routes -
/// `/` is the projects index, `/log` the event log - so a reader who
/// reloads on one of them, or follows a link to one, must be served
/// the page rather than a 404 from a server that has never heard of
/// it.
async fn page(uri: Uri) -> Response {
    let path = uri.path();
    if path == "/api" || path.starts_with("/api/") {
        return (StatusCode::NOT_FOUND, format!("no endpoint at {path}")).into_response();
    }
    Html(PAGE).into_response()
}

/// `GET /api/events`: the project's log, filtered by the query string
/// `server::events::parse` reads the same way `cli::parse_query` reads
/// `SearchArgs`, scoped always to `state.source.path`.
async fn api_events(State(state): State<Arc<AppState>>, Query(params): Query<events::Params>) -> Response {
    let result = tokio::task::spawn_blocking(move || {
        let root = state.source.path.clone();
        events::list(&*state.log, params, root)
    })
    .await
    .expect("api_events's blocking read never panics");
    events_response(result)
}

/// `GET /api/events/{id}`: the whole wire event `id` names, in this
/// project, or a 404 when it names none.
async fn api_event(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let result = tokio::task::spawn_blocking(move || events::get(&*state.log, &id, &state.source.path))
        .await
        .expect("api_event's blocking read never panics");
    events_response(result)
}

/// `GET /api/projects`: every project the log holds, across the whole
/// log under `PERCEPT_HOME` - not scoped to `state.source.path`, the
/// one route on this server that answers for more than its own
/// project.
async fn api_projects(State(state): State<Arc<AppState>>) -> Response {
    let result = tokio::task::spawn_blocking(move || projects::list(&*state.log, state.opened))
        .await
        .expect("api_projects's blocking read never panics");
    events_response(result)
}

/// `GET /api/maps/{name}`: `name`'s map, cut to `params`'s `root`,
/// `around`, and `depth` - `root` names the project, any this log
/// holds, not only `state.source.path`.
async fn api_maps(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<maps::Params>,
) -> Response {
    let result = tokio::task::spawn_blocking(move || maps::get(&*state.log, &name, params))
        .await
        .expect("api_maps's blocking read never panics");
    events_response(result)
}

/// Turns `server::events`' result into the response every route in this
/// module shares: the body as JSON, or the error's reason as plain text
/// with the status its variant earns.
fn events_response(result: Result<serde_json::Value, EventsError>) -> Response {
    match result {
        Ok(body) => Json(body).into_response(),
        Err(EventsError::Bad(reason)) => (StatusCode::BAD_REQUEST, reason).into_response(),
        Err(EventsError::NotFound(reason)) => (StatusCode::NOT_FOUND, reason).into_response(),
        Err(EventsError::Internal(reason)) => (StatusCode::INTERNAL_SERVER_ERROR, reason).into_response(),
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
