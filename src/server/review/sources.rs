//! What a row's `sources` resolve to: the events a node's `sources`
//! names, as `GET /api/review` sends them - a message with the
//! proposal a human's "yes" answered, a file's path and excerpt,
//! anything else by its type, or `missing` when the log no longer
//! holds it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::{cited_label, Actor, Event, EventId, Payload};
use crate::mapstore::citation::locate;
use crate::workspace;

/// A source entry's `content` or `excerpt` is sent whole up to this
/// many characters; beyond it the entry is cut at a character boundary
/// and marked `truncated`. The log is large and a row's exchange can
/// quote a long reply or a long file - this keeps one entry from
/// blowing up a request that lists many rows.
const MAX_SOURCE_CHARS: usize = 4000;

/// The proposal a human prompt's "yes" answered: the latest agent
/// reply in the same source before it.
#[derive(Serialize, Clone)]
pub struct Proposal {
    id: String,
    at: String,
    content: String,
}

/// One event a node's `sources` names, as `GET /api/review` resolves
/// it - what the source line folds open to.
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SourceEntry {
    Message {
        id: String,
        at: String,
        actor: String,
        client: String,
        content: String,
        truncated: bool,
        proposal: Option<Proposal>,
    },
    File {
        id: String,
        at: String,
        path: String,
        lines: Option<(u32, u32)>,
        label: String,
        excerpt: String,
        truncated: bool,
    },
    Event {
        id: String,
        at: String,
        #[serde(rename = "type")]
        kind: String,
    },
    Missing {
        id: String,
    },
}

/// `text` cut to `MAX_SOURCE_CHARS`, and whether it was.
fn truncate(text: &str) -> (String, bool) {
    match text.char_indices().nth(MAX_SOURCE_CHARS) {
        Some((at, _)) => (text[..at].to_string(), true),
        None => (text.to_string(), false),
    }
}

/// Every event loaded for one request, indexed by id, and every agent
/// `message.received` indexed by its source, both built in one pass so
/// resolving a node's `sources` never scans the whole log per source -
/// the log is large and a request lists many rows. `entry` memoises
/// what it builds, since two nodes can cite the same event.
pub struct EventIndex<'a> {
    root: &'a Path,
    by_id: HashMap<EventId, &'a Event>,
    agent_replies: HashMap<(String, PathBuf), Vec<&'a Event>>,
    cache: RefCell<HashMap<EventId, SourceEntry>>,
    text_cache: RefCell<HashMap<PathBuf, Option<String>>>,
}

impl<'a> EventIndex<'a> {
    pub fn new(events: &'a [Event], root: &'a Path) -> Self {
        let mut by_id = HashMap::new();
        let mut agent_replies: HashMap<(String, PathBuf), Vec<&Event>> = HashMap::new();
        for event in events {
            by_id.insert(event.id(), event);
            if matches!(event.actor(), Actor::Agent)
                && matches!(event.payload(), Payload::MessageReceived { .. })
            {
                let key = (event.source().name.clone(), event.source().path.clone());
                agent_replies.entry(key).or_default().push(event);
            }
        }
        Self {
            root,
            by_id,
            agent_replies,
            cache: RefCell::new(HashMap::new()),
            text_cache: RefCell::new(HashMap::new()),
        }
    }

    /// `path`'s text under `root` now, read once per path and memoised
    /// for the rest of this index's life - `None` when it is missing,
    /// binary, or outside the checkout. The path comes off a log line
    /// another writer may have appended, so it becomes a real path the
    /// one way every other path does, through `Workspace`.
    fn tree_text(&self, path: &Path) -> Option<String> {
        if let Some(cached) = self.text_cache.borrow().get(path) {
            return cached.clone();
        }
        let text = workspace::Workspace::new(self.root)
            .ok()
            .and_then(|workspace| workspace.resolve(&path.to_string_lossy()).ok())
            .and_then(|resolved| workspace::read_text_lossy(&resolved).ok());
        self.text_cache.borrow_mut().insert(path.to_path_buf(), text.clone());
        text
    }

    /// The latest agent reply in the same source as `message` and
    /// before it - the proposal a human's "yes" answered, or `None`
    /// when there is none.
    fn proposal_of(&self, message: &Event) -> Option<Proposal> {
        let key = (message.source().name.clone(), message.source().path.clone());
        let replies = self.agent_replies.get(&key)?;
        let at = replies.partition_point(|event| event.created_at() < message.created_at());
        let event = *replies[..at].last()?;
        let Payload::MessageReceived { content } = event.payload() else {
            return None;
        };
        let (content, _) = truncate(content);
        Some(Proposal {
            id: event.id().as_uuid().to_string(),
            at: event.created_at().to_string(),
            content,
        })
    }

    /// One entry of a node's `sources`: the event `id` names, read as
    /// what a source line documents - a message, a file, anything
    /// else, or `missing` when the log no longer holds it.
    fn build(&self, id: EventId) -> SourceEntry {
        let Some(event) = self.by_id.get(&id) else {
            return SourceEntry::Missing { id: id.as_uuid().to_string() };
        };
        let entry_id = event.id().as_uuid().to_string();
        let at = event.created_at().to_string();
        match event.payload() {
            Payload::MessageReceived { content } => {
                let (content, truncated) = truncate(content);
                let actor = match event.actor() {
                    // A system-authored message reads as the human's
                    // own: percept itself never speaks as a proposal or
                    // reply.
                    Actor::System => "human".to_string(),
                    other => other.name().to_string(),
                };
                let proposal = match event.actor() {
                    Actor::Agent => None,
                    _ => self.proposal_of(event),
                };
                SourceEntry::Message {
                    id: entry_id,
                    at,
                    actor,
                    client: event.source().name.clone(),
                    content,
                    truncated,
                    proposal,
                }
            }
            Payload::FileCited { path, excerpt, .. } => {
                let lines = self.tree_text(path).and_then(|text| locate(excerpt, &text));
                let (excerpt, truncated) = truncate(excerpt);
                SourceEntry::File {
                    id: entry_id,
                    at,
                    path: path.to_string_lossy().into_owned(),
                    lines,
                    label: cited_label(path, lines),
                    excerpt,
                    truncated,
                }
            }
            _ => SourceEntry::Event {
                id: entry_id,
                at,
                kind: event.kind().name().to_string(),
            },
        }
    }

    fn entry(&self, id: EventId) -> SourceEntry {
        if let Some(cached) = self.cache.borrow().get(&id) {
            return cached.clone();
        }
        let built = self.build(id);
        self.cache.borrow_mut().insert(id, built.clone());
        built
    }

    /// A node's `sources`, one entry per id in order.
    pub fn sources_json(&self, sources: &[EventId]) -> Vec<SourceEntry> {
        sources.iter().map(|id| self.entry(*id)).collect()
    }
}
