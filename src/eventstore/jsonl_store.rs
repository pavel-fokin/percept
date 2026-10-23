use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Mutex;

use tokio::io::AsyncWriteExt;

use super::line::EventLine;
use crate::core::{ClientSessionId, Event, EventLog, EventStore, Payload, SessionId};
use crate::shared::Result;

pub struct JsonlStore {
    path: PathBuf,
}

impl JsonlStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

/// The log read once under the advisory lock; closing the lock file on drop releases it.
pub struct Transaction {
    path: PathBuf,
    events: Mutex<Vec<Event>>,
    _lock: std::fs::File,
}

impl EventLog for JsonlStore {
    type Transaction = Transaction;

    async fn begin(&self) -> Result<Transaction> {
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

        let lock = tokio::task::spawn_blocking(acquire)
            .await?
            .map_err(|error| format!("{}.lock: {error}", self.path.display()))?;

        let events = self.read().await?;

        Ok(Transaction { path: self.path.clone(), events: Mutex::new(events), _lock: lock })
    }
}

impl JsonlStore {
    async fn read(&self) -> Result<Vec<Event>> {
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
}

impl EventStore for Transaction {
    async fn session(
        &self,
        client_session_id: &ClientSessionId,
    ) -> Result<Option<SessionId>> {
        let session = self
            .events
            .lock()
            .unwrap()
            .iter()
            .find_map(|e| match &e.payload {
                Payload::SessionCreated { session, client_session_id: id, .. } if id == client_session_id => {
                    Some(*session)
                }
                _ => None,
            });

        Ok(session)
    }

    async fn all(&self) -> Result<Vec<Event>> {
        Ok(self.events.lock().unwrap().clone())
    }

    async fn append(&self, event: &Event) -> Result<()> {
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
            .map_err(|error| format!("{}: {error}", path.display()))?;
        self.events.lock().unwrap().push(event.clone());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::core::{Actor, EventId, Kind, ProjectId, Source};
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
        assert_eq!(file.store().begin().await.unwrap().all().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn empty_file_has_no_events() {
        let file = TempFile::new();
        file.write("");
        assert_eq!(file.store().begin().await.unwrap().all().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn reads_one_event_per_line() {
        let file = TempFile::new();
        let events: Vec<Event> = (0..3).map(|_| Event::tool_used(Source::fixture(), SessionId::new(), "Bash", None, None, json!({}))).collect();
        let ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
        let text: String = events
            .iter()
            .map(|event| serde_json::to_string(&EventLine::from(event)).unwrap() + "\n")
            .collect();
        file.write(&text);

        let events = file.store().begin().await.unwrap().all().await.unwrap();

        assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), ids);
    }

    #[tokio::test]
    async fn append_creates_the_file_and_round_trips() {
        let file = TempFile::new();
        let transaction = file.store().begin().await.unwrap();
        transaction
            .append(&Event::message(Source::fixture(), SessionId::new(), Actor::Human, "hi", json!({"a": 1})))
            .await
            .unwrap();
        transaction
            .append(&Event::session_started(Source::fixture(), SessionId::new(), json!({"b": 2})))
            .await
            .unwrap();
        drop(transaction);

        let events = file.store().begin().await.unwrap().all().await.unwrap();

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
        let transaction = nested.begin().await.unwrap();
        transaction.append(&Event::tool_used(Source::fixture(), SessionId::new(), "Bash", None, None, json!(null))).await.unwrap();
        assert_eq!(transaction.all().await.unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir.0);
    }

    #[tokio::test]
    async fn session_finds_a_created_session_and_none_for_an_unknown_client_session_id() {
        let file = TempFile::new();
        let transaction = file.store().begin().await.unwrap();
        let created = Event::session_created(Source::fixture(), SessionId::new(), ProjectId::new(), ClientSessionId::new("a".into()));
        let session = created.payload.session().unwrap();
        transaction.append(&created).await.unwrap();

        let found = transaction.session(&ClientSessionId::new("a".into())).await.unwrap();
        let unknown = transaction.session(&ClientSessionId::new("b".into())).await.unwrap();

        assert_eq!(found, Some(session));
        assert_eq!(unknown, None);
    }

    #[tokio::test]
    async fn a_transaction_reads_the_file_once() {
        let file = TempFile::new();
        let transaction = file.store().begin().await.unwrap();

        file.write(&(serde_json::to_string(&EventLine::from(&Event::tool_used(Source::fixture(), SessionId::new(), "Bash", None, None, json!({})))).unwrap() + "\n"));

        assert_eq!(transaction.all().await.unwrap().len(), 0);
        assert_eq!(transaction.all().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn an_append_is_visible_in_the_transaction_and_in_the_file() {
        let file = TempFile::new();
        let transaction = file.store().begin().await.unwrap();
        let event = Event::tool_used(Source::fixture(), SessionId::new(), "Bash", None, None, json!({}));

        transaction.append(&event).await.unwrap();

        assert_eq!(transaction.all().await.unwrap()[0].id, event.id);
        assert_eq!(fs::read_to_string(&file.0).unwrap().lines().count(), 1);
    }

    #[tokio::test]
    async fn begin_creates_a_sibling_lock_file_and_waits_for_the_open_transaction() {
        let file = TempFile::new();
        let store = Arc::new(file.store());
        let lock_path = PathBuf::from(format!("{}.lock", file.0.display()));
        let first = store.begin().await.unwrap();

        let waiting = tokio::spawn({
            let store = store.clone();
            async move { store.begin().await.map(|_| ()) }
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let waited = !waiting.is_finished();
        drop(first);
        let second = waiting.await.unwrap();

        assert!(lock_path.exists());
        assert!(waited);
        assert!(second.is_ok());
        let _ = fs::remove_file(lock_path);
    }

    #[tokio::test]
    async fn malformed_line_is_an_error() {
        let file = TempFile::new();
        file.write("not json\n");
        assert!(file.store().begin().await.is_err());
    }
}
