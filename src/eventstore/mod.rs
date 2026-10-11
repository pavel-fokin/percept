//! Infrastructure layer: stores and queries Events as JSONL.

use std::error::Error;
use std::io::ErrorKind;
use std::path::PathBuf;

use jiff::Timestamp;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::core::{Actor, ClientSessionId, Event, EventId, EventStore, Kind, Payload, SessionId, Source};

pub struct JsonlStore {
    path: PathBuf,
}

impl JsonlStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

/// One log line. The log format lives here, apart from the domain `Event`.
#[derive(Serialize, Deserialize)]
struct EventLine {
    id: EventId,
    source: Source,
    actor: Actor,
    kind: String,
    payload: serde_json::Value,
    raw: Option<serde_json::Value>,
    created_at: Timestamp,
}

impl From<&Event> for EventLine {
    fn from(event: &Event) -> Self {
        let payload = match &event.payload {
            Payload::SessionCreated { session, client_session_id } => {
                json!({ "session": session, "client_session_id": client_session_id.as_str() })
            }
            Payload::SessionStarted { session } | Payload::SessionStopped { session } => {
                json!({ "session": session })
            }
            Payload::Message { session, content } => json!({ "session": session, "content": content }),
            Payload::ToolUsed { session, tool } => json!({ "session": session, "tool": tool }),
        };

        Self {
            id: event.id,
            source: event.source.clone(),
            actor: event.actor,
            kind: event.payload.kind().name().into(),
            payload,
            raw: event.raw.clone(),
            created_at: event.created_at,
        }
    }
}

impl TryFrom<EventLine> for Event {
    type Error = Box<dyn Error + Send + Sync>;

    fn try_from(line: EventLine) -> Result<Self, Self::Error> {
        let kind = Kind::from_name(&line.kind).ok_or_else(|| format!("unknown event kind {:?}", line.kind))?;

        let p = &line.payload;
        let payload = match kind {
            Kind::SessionCreated => Payload::SessionCreated {
                session: field(p, "session")?,
                client_session_id: ClientSessionId::new(field(p, "client_session_id")?),
            },
            Kind::SessionStarted => Payload::SessionStarted { session: field(p, "session")? },
            Kind::SessionStopped => Payload::SessionStopped { session: field(p, "session")? },
            Kind::Message => Payload::Message { session: field(p, "session")?, content: field(p, "content")? },
            Kind::ToolUsed => Payload::ToolUsed { session: field(p, "session")?, tool: field(p, "tool")? },
        };

        Ok(Event {
            id: line.id,
            source: line.source,
            actor: line.actor,
            payload,
            raw: line.raw,
            created_at: line.created_at,
        })
    }
}

fn field<T: DeserializeOwned>(
    payload: &serde_json::Value,
    name: &str,
) -> Result<T, Box<dyn Error + Send + Sync>> {
    serde_json::from_value(payload[name].clone())
        .map_err(|error| format!("payload.{name}: {error}").into())
}

/// Holds the advisory lock; closing the file on drop releases it.
pub struct Lock {
    _file: std::fs::File,
}

impl EventStore for JsonlStore {
    type Lock = Lock;

    async fn lock(&self) -> Result<Lock, Box<dyn Error + Send + Sync>> {
        let mut name = self.path.clone().into_os_string();
        name.push(".lock");
        let path = PathBuf::from(name);

        let acquire = move || -> std::io::Result<std::fs::File> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }

            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&path)?;
            file.lock()?;

            Ok(file)
        };

        let file = tokio::task::spawn_blocking(acquire)
            .await?
            .map_err(|error| format!("{}.lock: {error}", self.path.display()))?;

        Ok(Lock { _file: file })
    }

    async fn session(
        &self,
        client_session_id: &ClientSessionId,
    ) -> Result<Option<SessionId>, Box<dyn Error + Send + Sync>> {
        let session = self
            .all()
            .await?
            .into_iter()
            .find_map(|e| match e.payload {
                Payload::SessionCreated { session, client_session_id: id } if id == *client_session_id => {
                    Some(session)
                }
                _ => None,
            });

        Ok(session)
    }

    async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
        let text = match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("{}: {error}", self.path.display()).into()),
        };
        text.lines()
            .enumerate()
            .map(|(index, line)| {
                let event = serde_json::from_str::<EventLine>(line).map_err(Into::into).and_then(Event::try_from);
                event.map_err(|error| {
                    format!("{}:{}: {error}", self.path.display(), index + 1).into()
                })
            })
            .collect()
    }

    async fn append(&self, event: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut line = serde_json::to_string(&EventLine::from(event))?;
        line.push('\n');

        let path = &self.path;
        let write = async {
            if let Some(dir) = path.parent() {
                tokio::fs::create_dir_all(dir).await?;
            }

            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await?;

            // One write call, so concurrent appends do not interleave.
            file.write_all(line.as_bytes()).await?;
            // tokio buffers the write; flush surfaces its error.
            file.flush().await
        };

        write
            .await
            .map_err(|error| format!("{}: {error}", path.display()).into())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::core::{Actor, EventId};
    use serde_json::json;

    struct TempFile(PathBuf);

    impl TempFile {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let name = format!(
                "percept-test-{}-{}.jsonl",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            Self(std::env::temp_dir().join(name))
        }

        fn write(&self, content: &str) {
            fs::write(&self.0, content).unwrap();
        }

        fn store(&self) -> JsonlStore {
            JsonlStore::new(self.0.clone())
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[tokio::test]
    async fn missing_file_has_no_events() {
        let file = TempFile::new();
        assert_eq!(file.store().all().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn empty_file_has_no_events() {
        let file = TempFile::new();
        file.write("");
        assert_eq!(file.store().all().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn reads_one_event_per_line() {
        let file = TempFile::new();
        let events: Vec<Event> = (0..3).map(|_| Event::tool_used(Source::fixture(), SessionId::new(), "Bash", json!({}))).collect();
        let ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
        let text: String = events
            .iter()
            .map(|event| serde_json::to_string(&EventLine::from(event)).unwrap() + "\n")
            .collect();
        file.write(&text);

        let events = file.store().all().await.unwrap();

        assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), ids);
    }

    #[tokio::test]
    async fn append_creates_the_file_and_round_trips() {
        let file = TempFile::new();
        let store = file.store();
        store
            .append(&Event::message(Source::fixture(), SessionId::new(), Actor::Human, "hi", json!({"a": 1})))
            .await
            .unwrap();
        store
            .append(&Event::session_started(Source::fixture(), SessionId::new(), json!({"b": 2})))
            .await
            .unwrap();

        let events = store.all().await.unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].actor, Actor::Human);
        assert_eq!(events[0].payload.kind(), Kind::Message);
        assert_eq!(events[0].raw, Some(json!({"a": 1})));
        assert_eq!(events[1].actor, Actor::System);
        assert_eq!(events[1].payload.kind(), Kind::SessionStarted);
        assert_eq!(events[1].raw, Some(json!({"b": 2})));
    }

    #[tokio::test]
    async fn append_creates_a_missing_directory() {
        let dir = TempFile::new();
        let nested = JsonlStore::new(dir.0.join("sub").join("log.jsonl"));
        nested.append(&Event::tool_used(Source::fixture(), SessionId::new(), "Bash", json!(null))).await.unwrap();
        assert_eq!(nested.all().await.unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir.0);
    }

    #[tokio::test]
    async fn session_finds_a_created_session_and_none_for_an_unknown_client_session_id() {
        let file = TempFile::new();
        let store = file.store();
        let created = Event::session_created(Source::fixture(), SessionId::new(), ClientSessionId::new("a".into()));
        let session = created.payload.session();
        store.append(&created).await.unwrap();

        let found = store.session(&ClientSessionId::new("a".into())).await.unwrap();
        let unknown = store.session(&ClientSessionId::new("b".into())).await.unwrap();

        assert_eq!(found, Some(session));
        assert_eq!(unknown, None);
    }

    #[tokio::test]
    async fn lock_creates_a_sibling_lock_file_and_can_be_retaken_after_drop() {
        let file = TempFile::new();
        let store = file.store();
        let lock_path = PathBuf::from(format!("{}.lock", file.0.display()));

        drop(store.lock().await.unwrap());
        let again = store.lock().await;

        assert!(lock_path.exists());
        assert!(again.is_ok());
        let _ = fs::remove_file(lock_path);
    }

    #[test]
    fn every_payload_round_trips_through_a_line() {
        let session = SessionId::new();
        let raw = json!({"r": 1});
        let events = [
            Event::session_created(Source::fixture(), session, ClientSessionId::new("a".into())),
            Event::session_started(Source::fixture(), session, raw.clone()),
            Event::session_stopped(Source::fixture(), session, raw.clone()),
            Event::message(Source::fixture(), session, Actor::Human, "hi", raw.clone()),
            Event::tool_used(Source::fixture(), session, "Bash", raw),
        ];

        for event in events {
            let text = serde_json::to_string(&EventLine::from(&event)).unwrap();
            let line: EventLine = serde_json::from_str(&text).unwrap();
            let back = Event::try_from(line).unwrap();

            assert_eq!(back.id, event.id);
            assert_eq!(back.source, event.source);
            assert_eq!(back.actor, event.actor);
            assert_eq!(back.payload, event.payload);
            assert_eq!(back.raw, event.raw);
            assert_eq!(back.created_at, event.created_at);
        }
    }

    #[test]
    fn a_line_has_a_kind_and_a_payload_naming_the_session() {
        let session = SessionId::new();
        let event = Event::message(Source::fixture(), session, Actor::Human, "hi", json!({"r": 1}));

        let line = serde_json::to_value(EventLine::from(&event)).unwrap();

        assert_eq!(line["source"], json!({"client": "claude", "path": "/repo"}));
        assert_eq!(line["actor"], "human");
        assert_eq!(line["kind"], "message");
        assert_eq!(line["payload"], json!({"session": session, "content": "hi"}));
        assert_eq!(line["raw"], json!({"r": 1}));
    }

    #[test]
    fn unknown_kind_is_an_error() {
        let event = Event::message(Source::fixture(), SessionId::new(), Actor::Human, "hi", json!({}));
        let mut line = EventLine::from(&event);
        line.kind = "bogus".into();

        assert!(Event::try_from(line).is_err());
    }

    #[tokio::test]
    async fn malformed_line_is_an_error() {
        let file = TempFile::new();
        file.write("not json\n");
        assert!(file.store().all().await.is_err());
    }
}
