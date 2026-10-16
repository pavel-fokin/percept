use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;

use crate::core::{ClientSessionId, EdgeKind, Event, EventStore, Payload, Schema, SchemaStore, SessionId, Source};
use crate::shared::Result;

pub(super) struct Fixed(pub(super) usize);

impl EventStore for Fixed {
    type Lock = ();

    async fn lock(&self) -> Result<()> {
        unimplemented!()
    }

    async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>> {
        unimplemented!()
    }

    async fn all(&self) -> Result<Vec<Event>> {
        Ok((0..self.0)
            .map(|_| Event::session_started(Source::fixture(), SessionId::new(), json!({})))
            .collect())
    }

    async fn append(&self, _: &[Event]) -> Result<()> {
        unimplemented!()
    }
}

pub(super) struct Broken;

impl EventStore for Broken {
    type Lock = ();

    async fn lock(&self) -> Result<()> {
        Err("boom".into())
    }

    async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>> {
        Err("boom".into())
    }

    async fn all(&self) -> Result<Vec<Event>> {
        Err("boom".into())
    }

    async fn append(&self, _: &[Event]) -> Result<()> {
        Err("boom".into())
    }
}

#[derive(Default)]
pub(super) struct Memory(pub(super) Mutex<Vec<Event>>);

impl EventStore for &Memory {
    type Lock = ();

    async fn lock(&self) -> Result<()> {
        Ok(())
    }

    async fn session(&self, client_session_id: &ClientSessionId) -> Result<Option<SessionId>> {
        let events = self.0.lock().unwrap();
        Ok(events
            .iter()
            .find_map(|e| match &e.payload {
                Payload::SessionCreated { session, client_session_id: id } if id == client_session_id => {
                    Some(*session)
                }
                _ => None,
            }))
    }

    async fn all(&self) -> Result<Vec<Event>> {
        let events = self.0.lock().unwrap();
        Ok(events.clone())
    }

    async fn append(&self, events: &[Event]) -> Result<()> {
        self.0.lock().unwrap().extend_from_slice(events);
        Ok(())
    }
}

/// Has a schema for the map kinds the app tests use, and none for any other.
pub(super) struct Schemas;

impl SchemaStore for Schemas {
    async fn schema(&self, _: &Path, kind: &str) -> Result<Schema> {
        match kind {
            "decisions" => mini(
                &[("question", &[]), ("option", &[])],
                &[("answers", "option", "question"), ("raises", "question", "option")],
            ),
            "tasks" => mini(&[("task", &[])], &[("blocks", "task", "task")]),
            "concepts" => mini(&[("concept", &["definition"])], &[]),
            _ => Err(format!("no schema for {kind}").into()),
        }
    }
}

fn mini(nodes: &[(&str, &[&str])], edges: &[(&str, &str, &str)]) -> Result<Schema> {
    let nodes = nodes
        .iter()
        .map(|(node, properties)| (node.to_string(), properties.iter().map(|p| p.to_string()).collect::<BTreeSet<_>>()))
        .collect();
    let edges = edges
        .iter()
        .map(|(edge, from, to)| (edge.to_string(), EdgeKind { from: from.to_string(), to: to.to_string() }))
        .collect();

    Schema::new("purpose".into(), "instructions".into(), nodes, edges)
}

/// Has no schema for any map kind.
pub(super) struct NoSchemas;

impl SchemaStore for NoSchemas {
    async fn schema(&self, _: &Path, kind: &str) -> Result<Schema> {
        Err(format!("no schema for {kind}").into())
    }
}

/// A folder under the system temp dir, removed on drop.
pub(super) struct TempDir(pub(super) PathBuf);

impl TempDir {
    pub(super) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!("percept-app-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap();

        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
