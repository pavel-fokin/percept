use std::sync::Arc;

use crate::percept::{self, Event, Sender};

/// What tui needs from the application layer. Owned here, not in tui:
/// Rust requires the implementor to import a trait to name it in an impl
/// block, so putting this in tui would force an app -> tui edge. tui
/// depends on this trait instead of the concrete Conversation type.
pub trait AppService {
    /// Records the user's message and returns a thunk that computes the
    /// assistant's reply. The history it needs is captured now, before
    /// returning - the thunk borrows nothing, so it's safe to run on any
    /// thread (e.g. via `spawn_blocking`).
    fn submit(&mut self, text: String) -> Box<dyn FnOnce() -> String + Send>;

    /// Records an assistant reply. Must only be called from the task
    /// that owns the Conversation - never from inside the thunk.
    fn append_reply(&mut self, content: String);

    fn events(&self) -> &[Event];
}

/// Conversation orchestrates a chat: turns input into domain events, asks
/// the configured Model for a reply, keeps the transcript. Pure
/// orchestration - no vocabulary beyond percept's.
pub struct Conversation {
    events: Vec<Event>,
    chat: Arc<dyn percept::Model>,
}

impl Conversation {
    pub fn new(chat: Arc<dyn percept::Model>) -> Self {
        Self {
            events: Vec::new(),
            chat,
        }
    }
}

impl AppService for Conversation {
    fn submit(&mut self, text: String) -> Box<dyn FnOnce() -> String + Send> {
        self.events.push(Event::new(Sender::User, text));

        let history = percept::to_messages(&self.events);
        let chat = Arc::clone(&self.chat);
        Box::new(move || match chat.reply(&history) {
            Ok(reply) => reply,
            Err(_) => "Sorry, something went wrong.".to_string(),
        })
    }

    fn append_reply(&mut self, content: String) {
        self.events.push(Event::new(Sender::Assistant, content));
    }

    fn events(&self) -> &[Event] {
        &self.events
    }
}
