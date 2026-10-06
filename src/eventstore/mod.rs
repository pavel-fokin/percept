//! Infrastructure layer: stores and queries Events as JSONL.

use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

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
    fn all(&self) -> Result<Vec<Event>, Box<dyn Error>> {
        let text = match fs::read_to_string(&self.path) {
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
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::shared::Id;

    /// A unique temp path that is removed on drop.
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

    #[test]
    fn missing_file_has_no_events() {
        let file = TempFile::new();
        assert_eq!(file.store().all().unwrap().len(), 0);
    }

    #[test]
    fn empty_file_has_no_events() {
        let file = TempFile::new();
        file.write("");
        assert_eq!(file.store().all().unwrap().len(), 0);
    }

    #[test]
    fn reads_one_event_per_line() {
        let file = TempFile::new();
        let ids: Vec<Id<Event>> = (0..3).map(|_| Id::new()).collect();
        let lines: Vec<String> = ids
            .iter()
            .map(|id| serde_json::to_string(&Event { id: *id }).unwrap())
            .collect();
        file.write(&(lines.join("\n") + "\n"));

        let events = file.store().all().unwrap();

        assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), ids);
    }

    #[test]
    fn malformed_line_is_an_error() {
        let file = TempFile::new();
        file.write("not json\n");
        assert!(file.store().all().is_err());
    }
}
