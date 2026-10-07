//! Infrastructure layer: stores and queries Events as JSONL.

use std::error::Error;
use std::io::ErrorKind;
use std::path::PathBuf;

use tokio::io::AsyncWriteExt;

use crate::core::{Event, EventStore};

pub struct JsonlStore {
    path: PathBuf,
}

impl JsonlStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl EventStore for JsonlStore {
    async fn all(&self) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
        let text = match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("{}: {error}", self.path.display()).into()),
        };
        text.lines()
            .enumerate()
            .map(|(index, line)| {
                serde_json::from_str(line).map_err(|error| {
                    format!("{}:{}: {error}", self.path.display(), index + 1).into()
                })
            })
            .collect()
    }

    async fn append(&self, event: &Event) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut line = serde_json::to_string(event)?;
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
        let events: Vec<Event> = (0..3).map(|_| Event::new(Actor::Agent, json!({}))).collect();
        let ids: Vec<EventId> = events.iter().map(|e| e.id).collect();
        let text: String = events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap() + "\n")
            .collect();
        file.write(&text);

        let events = file.store().all().await.unwrap();

        assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), ids);
    }

    #[tokio::test]
    async fn append_creates_the_file_and_round_trips() {
        let file = TempFile::new();
        let store = file.store();
        for (actor, payload) in [(Actor::Human, json!({"a": 1})), (Actor::System, json!([2]))] {
            store.append(&Event::new(actor, payload)).await.unwrap();
        }

        let events = store.all().await.unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].actor, Actor::Human);
        assert_eq!(events[0].payload, json!({"a": 1}));
        assert_eq!(events[1].actor, Actor::System);
        assert_eq!(events[1].payload, json!([2]));
    }

    #[tokio::test]
    async fn append_creates_a_missing_directory() {
        let dir = TempFile::new();
        let nested = JsonlStore::new(dir.0.join("sub").join("log.jsonl"));
        nested.append(&Event::new(Actor::Agent, json!(null))).await.unwrap();
        assert_eq!(nested.all().await.unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir.0);
    }

    #[tokio::test]
    async fn malformed_line_is_an_error() {
        let file = TempFile::new();
        file.write("not json\n");
        assert!(file.store().all().await.is_err());
    }
}
