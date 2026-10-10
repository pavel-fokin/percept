//! Server layer: serves the embedded page and its JSON API over HTTP.

use std::error::Error;
use std::process::Command;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tokio::net::TcpListener;

use crate::app::AppService;
use crate::core::EventStore;

const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

pub async fn serve<S>(service: Arc<AppService<S>>) -> Result<(), Box<dyn Error + Send + Sync>>
where
    S: EventStore + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    println!("percept • {url}");

    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(opener).arg(&url).spawn();

    axum::serve(listener, router(service)).await?;
    Ok(())
}

fn router<S>(service: Arc<AppService<S>>) -> Router
where
    S: EventStore + Send + Sync + 'static,
{
    Router::new()
        .route("/api/status", get(status))
        .route("/api/sessions", get(sessions))
        .fallback(fallback)
        .with_state(service)
}

async fn status<S: EventStore + Send + Sync + 'static>(
    State(service): State<Arc<AppService<S>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let events = service
        .event_count()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "data": { "events": events } })))
}

async fn sessions<S: EventStore + Send + Sync + 'static>(
    State(service): State<Arc<AppService<S>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sessions = service
        .sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "data": sessions })))
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use crate::core::{Event, SessionId, SessionKey};

    struct Fixed(usize);

    impl EventStore for Fixed {
        type Lock = ();

        async fn lock(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }

        async fn session(&self, _: &SessionKey) -> Result<Option<SessionId>, Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }

        async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
            let created = Event::session_created(SessionKey::new("k".into()));
            let session = created.session;
            let started = (1..self.0).map(move |_| Event::session_started(session, json!({})));
            Ok(std::iter::once(created).chain(started).collect())
        }

        async fn append(&self, _: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
            unimplemented!()
        }
    }

    async fn get_response(path: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(Arc::new(AppService::new(Fixed(3))));
        tokio::spawn(async move { axum::serve(listener, app).await });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn status_returns_the_event_count() {
        let response = get_response("/api/status").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.ends_with(r#"{"data":{"events":3}}"#));
    }

    #[tokio::test]
    async fn sessions_returns_id_and_key_per_session() {
        let response = get_response("/api/sessions").await;

        assert!(response.starts_with("HTTP/1.1 200"));
        let body: serde_json::Value = serde_json::from_str(response.rsplit("\r\n\r\n").next().unwrap()).unwrap();
        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["key"], "k");
        assert_eq!(data[0].as_object().unwrap().len(), 4);
        assert!(data[0]["id"].is_string());
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
}
