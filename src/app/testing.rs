use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;

use serde_json::json;

use crate::core::{Actor, ClientSessionId, Definition, DefinitionStore, EdgeKind, Event, EventStore, Name, Payload, Project, ProjectId, Schema, SessionId, Source};
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

    async fn append(&self, _: &Event) -> Result<()> {
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

    async fn append(&self, _: &Event) -> Result<()> {
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
                Payload::SessionCreated { session, client_session_id: id, .. } if id == client_session_id => {
                    Some(*session)
                }
                _ => None,
            }))
    }

    async fn all(&self) -> Result<Vec<Event>> {
        let events = self.0.lock().unwrap();
        Ok(events.clone())
    }

    async fn append(&self, event: &Event) -> Result<()> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
}

/// A log holding one project, rooted at `root`.
pub(super) fn in_project(root: &str) -> Memory {
    let store = Memory::default();
    add_project(&store, root);

    store
}

/// Appends a project rooted at `root`, named by a number so names never clash.
pub(super) fn add_project(store: &Memory, root: &str) {
    let source = Source::Checkout { client: "percept-cli".into(), path: root.into() };
    let name = Name::new(&format!("p{}", store.0.lock().unwrap().len())).unwrap();

    store.0.lock().unwrap().push(Event::project_created(source, Actor::Human, ProjectId::new(), name, Some(root.into())));
}

/// The id of the project nearest above `root`.
pub(super) fn id_at(store: &Memory, root: &str) -> ProjectId {
    Project::containing(&store.0.lock().unwrap(), Path::new(root)).unwrap().id
}

/// Appends a session of the project nearest above `root` and returns its id.
pub(super) fn add_session(store: &Memory, root: &str) -> SessionId {
    let mut events = store.0.lock().unwrap();
    let project = Project::containing(&events, Path::new(root)).unwrap().id;

    let session = SessionId::new();
    let source = Source::Checkout { client: "claude-code".into(), path: root.into() };
    let client_session_id = ClientSessionId::new(events.len().to_string());
    events.push(Event::session_created(source, session, project, client_session_id));

    session
}

/// Has a definition for the files the app tests use, keyed by file stem, and none for any other.
pub(super) struct Definitions;

impl DefinitionStore for Definitions {
    async fn definition(&self, file: &Path) -> Result<Definition> {
        match file.file_stem().and_then(|s| s.to_str()).unwrap_or_default() {
            "decisions" => mini(
                &[("question", &[]), ("option", &[])],
                &[("answers", "option", "question"), ("raises", "question", "option")],
            ),
            "tasks" => mini(&[("task", &[])], &[("blocks", "task", "task")]),
            "concepts" => mini(&[("concept", &["definition"])], &[]),
            "glossary" => mini(&[("term", &["definition", "source"])], &[]),
            _ => Err(format!("no definition in {}", file.display()).into()),
        }
    }
}

fn mini(nodes: &[(&str, &[&str])], edges: &[(&str, &str, &str)]) -> Result<Definition> {
    let nodes = nodes
        .iter()
        .map(|(node, properties)| (node.to_string(), properties.iter().map(|p| p.to_string()).collect::<BTreeSet<_>>()))
        .collect();
    let edges = edges
        .iter()
        .map(|(edge, from, to)| (edge.to_string(), EdgeKind { from: from.to_string(), to: to.to_string() }))
        .collect();

    Ok(Definition::new("purpose".into(), "instructions".into(), Schema::new(nodes, edges)?))
}

/// Has no definition for any file.
pub(super) struct NoDefinitions;

impl DefinitionStore for NoDefinitions {
    async fn definition(&self, file: &Path) -> Result<Definition> {
        Err(format!("no definition in {}", file.display()).into())
    }
}
